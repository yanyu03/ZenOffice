use crate::model::*;
use crate::{Result, SlideError};
use zen_opc::{Package, ZipArchive};
use std::path::Path;

pub fn read_pptx(path: impl AsRef<Path>) -> Result<Presentation> {
    let bytes = std::fs::read(path)?;
    read_pptx_bytes(&bytes)
}

pub fn read_pptx_bytes(bytes: &[u8]) -> Result<Presentation> {
    let archive = ZipArchive::read(bytes)?;
    let package = Package::from_archive(&archive)?;

    let mut pres = Presentation::default();
    let mut text_locations = Vec::new();

    // 读 core.xml 元数据
    if let Some(core_text) = package.get_text("/docProps/core.xml") {
        if let Ok(doc) = roxmltree::Document::parse(&core_text) {
            for node in doc.descendants() {
                if node.tag_name().name() == "title" {
                    if let Some(t) = node.text() {
                        pres.title = t.trim().to_string();
                    }
                } else if node.tag_name().name() == "creator" {
                    if let Some(c) = node.text() {
                        pres.author = c.trim().to_string();
                    }
                }
            }
        }
    }

    // 读 presentation.xml 确定尺寸与关系
    let root_rels = package.try_root_rels()?;
    let pres_part = root_rels
        .by_type(crate::write::REL_OFFICE_DOCUMENT)
        .and_then(|r| zen_opc::resolve_target("/", &r.target).ok())
        .unwrap_or_else(|| "/ppt/presentation.xml".to_string());

    let pres_text = package.get_text(&pres_part).ok_or_else(|| {
        SlideError::Invalid(format!("缺少主演示文稿部件: {pres_part}"))
    })?;
    let pres_doc = roxmltree::Document::parse(&pres_text)?;

    if let Some(sz_node) = pres_doc.descendants().find(|n| n.tag_name().name() == "sldSz") {
        if let Some(cx) = sz_node.attribute("cx").and_then(|v| v.parse::<i64>().ok()) {
            pres.width_emu = cx;
        }
        if let Some(cy) = sz_node.attribute("cy").and_then(|v| v.parse::<i64>().ok()) {
            pres.height_emu = cy;
        }
    }

    // 关系驱动：解析 presentation.xml.rels 与 <p:sldIdLst>
    let pres_rels = package.try_rels_for(&pres_part)?;
    let mut slide_parts: Vec<String> = Vec::new();

    if let Some(lst_node) = pres_doc.descendants().find(|n| n.tag_name().name() == "sldIdLst") {
        for sld_id_node in lst_node.children().filter(|n| n.tag_name().name() == "sldId") {
            let r_id = sld_id_node
                .attribute((crate::write::R_NS, "id"))
                .or_else(|| sld_id_node.attribute("r:id"))
                .or_else(|| {
                    sld_id_node
                        .attributes()
                        .find(|a| a.name() == "id" && a.namespace() == Some(crate::write::R_NS))
                        .map(|a| a.value())
                });

            if let Some(rid) = r_id {
                if let Some(rel) = pres_rels.get(rid) {
                    if rel.ty != crate::write::REL_SLIDE {
                        return Err(SlideError::Invalid(format!(
                            "幻灯片清单中 r:id={rid} 的关系类型不是 slide ({}), 实际为: {}",
                            crate::write::REL_SLIDE,
                            rel.ty
                        )));
                    }
                    let target_part = zen_opc::resolve_target(&pres_part, &rel.target)?;
                    slide_parts.push(target_part);
                } else {
                    return Err(SlideError::Invalid(format!(
                        "幻灯片清单中 r:id={rid} 未在关系表中定义"
                    )));
                }
            }
        }
    }

    // 兜底一：若未显式包含 sldIdLst，按 rels 关系类型查找
    if slide_parts.is_empty() {
        for rel in &pres_rels.items {
            if rel.ty == crate::write::REL_SLIDE {
                if let Ok(target_part) = zen_opc::resolve_target(&pres_part, &rel.target) {
                    slide_parts.push(target_part);
                }
            }
        }
    }

    // 兜底二：按包内名称自然排序兜底（仅用于部分残缺文件容错）
    if slide_parts.is_empty() {
        let mut names: Vec<String> = package
            .part_names()
            .into_iter()
            .filter(|n| n.starts_with("/ppt/slides/slide") && n.ends_with(".xml"))
            .map(|s| s.to_string())
            .collect();
        names.sort();
        slide_parts = names;
    }

    // 依次解析幻灯片，遇到损坏 XML 严禁静默吞错
    for part_name in slide_parts {
        let slide_text = package.get_text(&part_name).ok_or_else(|| {
            SlideError::Invalid(format!("引用的幻灯片部件丢失: {part_name}"))
        })?;

        let doc = roxmltree::Document::parse(&slide_text)?;
        let mut slide = Slide::new();

        // 收集未投影的形状（连接符、原生表格/图表图形框、图片、组合形状等），记录明确损失诊断
        for node in doc.descendants().filter(|n| n.is_element()) {
            let tag = node.tag_name().name();
            match tag {
                "cxnSp" => {
                    let id = node.descendants().find(|n| n.tag_name().name() == "cNvPr").and_then(|n| n.attribute("id")).unwrap_or("?");
                    let name = node.descendants().find(|n| n.tag_name().name() == "cNvPr").and_then(|n| n.attribute("name")).unwrap_or("Connector");
                    pres.diagnostics.push(SlideDiagnostic {
                        part: part_name.clone(),
                        element: "cxnSp".into(),
                        detail: format!("连接符形状 [id: {id}, name: '{name}'] 未投影到当前模型；原包保存保留，直接编辑暂不支持"),
                    });
                }
                "graphicFrame" => {
                    let id = node.descendants().find(|n| n.tag_name().name() == "cNvPr").and_then(|n| n.attribute("id")).unwrap_or("?");
                    let name = node.descendants().find(|n| n.tag_name().name() == "cNvPr").and_then(|n| n.attribute("name")).unwrap_or("GraphicFrame");
                    pres.diagnostics.push(SlideDiagnostic {
                        part: part_name.clone(),
                        element: "graphicFrame".into(),
                        detail: format!("原生表格/图形框 [id: {id}, name: '{name}'] 未投影到当前模型；原包保存保留，直接编辑暂不支持"),
                    });
                }
                "pic" => {
                    let id = node.descendants().find(|n| n.tag_name().name() == "cNvPr").and_then(|n| n.attribute("id")).unwrap_or("?");
                    let name = node.descendants().find(|n| n.tag_name().name() == "cNvPr").and_then(|n| n.attribute("name")).unwrap_or("Picture");
                    pres.diagnostics.push(SlideDiagnostic {
                        part: part_name.clone(),
                        element: "pic".into(),
                        detail: format!("图片形状 [id: {id}, name: '{name}'] 未投影到当前模型；原包保存保留，直接编辑暂不支持"),
                    });
                }
                "grpSp" => {
                    let id = node.descendants().find(|n| n.tag_name().name() == "cNvPr").and_then(|n| n.attribute("id")).unwrap_or("?");
                    let name = node.descendants().find(|n| n.tag_name().name() == "cNvPr").and_then(|n| n.attribute("name")).unwrap_or("GroupShape");
                    pres.diagnostics.push(SlideDiagnostic {
                        part: part_name.clone(),
                        element: "grpSp".into(),
                        detail: format!("组合形状 [id: {id}, name: '{name}'] 未投影到当前模型；原包保存保留，结构编辑暂不支持"),
                    });
                }
                _ => {}
            }
        }

        for sp_node in doc.descendants().filter(|n| n.tag_name().name() == "sp") {
            let mut shape = Shape::textbox(0, 0, 0, 0);

            if let Some(nv) = sp_node.descendants().find(|n| n.tag_name().name() == "cNvPr") {
                if let Some(id_str) = nv.attribute("id").and_then(|v| v.parse::<u32>().ok()) {
                    shape.id = id_str;
                }
                if let Some(name) = nv.attribute("name") {
                    shape.name = name.to_string();
                }
            }

            // 读几何与坐标
            if let Some(xfrm) = sp_node.descendants().find(|n| n.tag_name().name() == "xfrm") {
                if let Some(off) = xfrm.children().find(|n| n.tag_name().name() == "off") {
                    shape.x = off.attribute("x").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
                    shape.y = off.attribute("y").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
                }
                if let Some(ext) = xfrm.children().find(|n| n.tag_name().name() == "ext") {
                    shape.width = ext.attribute("cx").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
                    shape.height = ext.attribute("cy").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
                }
            }
            // A placeholder may inherit its transform from the slide layout
            // and then the master. Do not interpret missing xfrm as a tiny box.
            if placeholder(sp_node).is_some() {
                let direct=transform(sp_node);
                let inherited=placeholder_transform(&package,&part_name,sp_node)?;
                shape.x=direct[0].or(inherited[0]).unwrap_or(0);
                shape.y=direct[1].or(inherited[1]).unwrap_or(0);
                shape.width=direct[2].or(inherited[2]).unwrap_or(0);
                shape.height=direct[3].or(inherited[3]).unwrap_or(0);
            }

            // 读几何预设与填充
            if let Some(geom) = sp_node.descendants().find(|n| n.tag_name().name() == "prstGeom") {
                if let Some(prst) = geom.attribute("prst") {
                    shape.kind = match prst {
                        "roundRect" => ShapeKind::RoundedRectangle,
                        "ellipse" => ShapeKind::Ellipse,
                        "rect" => {
                            if sp_node.descendants().any(|n| n.tag_name().name() == "cNvSpPr" && n.attribute("txBox") == Some("1")) {
                                ShapeKind::TextBox
                            } else {
                                ShapeKind::Rectangle
                            }
                        }
                        _ => ShapeKind::Rectangle,
                    };
                }
            }

            if let Some(sp_pr) = sp_node.children().find(|n| n.tag_name().name() == "spPr") {
                if let Some(clr) = sp_pr.descendants().find(|n| n.tag_name().name() == "srgbClr") {
                    if let Some(val) = clr.attribute("val") {
                        shape.fill_color = Some(val.to_string());
                    }
                }
            }

            // 读文字主体
            if let Some(tx_body) = sp_node.children().find(|n| n.tag_name().name() == "txBody") {
                let mut text_body = TextBody::default();
                for p_node in tx_body.children().filter(|n| n.tag_name().name() == "p") {
                    let mut para = Paragraph::default();
                    for r_node in p_node.children().filter(|n| n.tag_name().name() == "r") {
                        let mut run = Run::default();
                        if let Some(r_pr) = r_node.children().find(|n| n.tag_name().name() == "rPr") {
                            run.bold = r_pr.attribute("b").map(|v| v == "1" || v == "true").unwrap_or(false);
                            run.italic = r_pr.attribute("i").map(|v| v == "1" || v == "true").unwrap_or(false);
                            run.size_hundredth_pt = r_pr.attribute("sz").and_then(|v| v.parse::<u32>().ok());
                        }
                        if let Some(t_node) = r_node.children().find(|n| n.tag_name().name() == "t") {
                            if let Some(text) = t_node.text() {
                                run.text = text.to_string();
                            }
                        }
                        if !run.text.is_empty() {
                            let text_node = r_node.children().find(|n| n.tag_name().name() == "t").unwrap();
                            text_locations.push(zen_opc::preserve::TextLocation {
                                part: part_name.clone(), range: text_node.range(),
                                editable: text_node.tag_name().namespace() == Some(crate::write::A_NS) && r_node.tag_name().namespace()==Some(crate::write::A_NS) && text_node.children().all(|n|n.is_text()) && text_node.children().count()<=1,
                            });
                            para.runs.push(run);
                        }
                    }
                    text_body.paragraphs.push(para);
                }
                shape.text_body = Some(text_body);
            }

            slide.add_shape(shape);
        }

        pres.add_slide(slide);
    }

    pres.preservation = Some(std::sync::Arc::new(crate::preserve::PreservedPresentation {
        bytes: std::sync::Arc::new(bytes.to_vec()), baseline: Box::new(pres.clone()), locations: text_locations,
    }));
    Ok(pres)
}

fn placeholder(node: roxmltree::Node<'_, '_>) -> Option<(String,Option<String>)> {
    node.children().find(|n| n.tag_name().name()=="nvSpPr")?
        .descendants().find(|n| n.has_tag_name((crate::write::P_NS,"ph")))
        .map(|p|(p.attribute("type").unwrap_or("body").to_string(),p.attribute("idx").map(str::to_string)))
}
fn transform(node: roxmltree::Node<'_, '_>) -> [Option<i64>;4] {
    let Some(t)=node.children().find(|n|n.has_tag_name((crate::write::P_NS,"spPr")))
        .and_then(|p|p.children().find(|n|n.has_tag_name((crate::write::A_NS,"xfrm")))) else { return [None;4]; };
    let off=t.children().find(|n|n.has_tag_name((crate::write::A_NS,"off")));
    let ext=t.children().find(|n|n.has_tag_name((crate::write::A_NS,"ext")));
    [off.and_then(|n|n.attribute("x")).and_then(|s|s.parse().ok()),
     off.and_then(|n|n.attribute("y")).and_then(|s|s.parse().ok()),
     ext.and_then(|n|n.attribute("cx")).and_then(|s|s.parse().ok()),
     ext.and_then(|n|n.attribute("cy")).and_then(|s|s.parse().ok())]
}
fn placeholder_transform(package:&Package,slide:&str,node:roxmltree::Node<'_, '_>) -> Result<[Option<i64>;4]> {
    let Some((mut kind,mut index))=placeholder(node) else { return Ok([None;4]); };
    let mut part=slide.to_string(); let mut values=[None;4];
    for relation in [crate::write::REL_SLIDE_LAYOUT,crate::write::REL_SLIDE_MASTER] {
        let rels=package.try_rels_for(&part)?;
        let Some(rel)=rels.by_type(relation) else { break; };
        if rel.mode.as_deref()==Some("External") { return Err(SlideError::Invalid("external placeholder inheritance".into())); }
        part=zen_opc::resolve_target(&part,&rel.target)?;
        let text=package.get_text(&part).ok_or_else(||SlideError::Invalid("placeholder layout/master missing".into()))?;
        let xml=roxmltree::Document::parse(&text)?;
        let candidates:Vec<_>=xml.descendants().filter(|n|n.has_tag_name((crate::write::P_NS,"sp"))).collect();
        let matched=candidates.iter().copied().find(|n|placeholder(*n).is_some_and(|(k,i)|i==index && k==kind))
            .or_else(||candidates.iter().copied().find(|n|placeholder(*n).is_some_and(|(k,i)|
                index.is_some() && i==index || index.is_none() && (k==kind || (matches!(kind.as_str(),"title"|"ctrTitle") && matches!(k.as_str(),"title"|"ctrTitle"))))));
        if let Some(shape)=matched {
            let next=transform(shape); for i in 0..4 { values[i]=values[i].or(next[i]); }
            if let Some((k,i))=placeholder(shape) { kind=k;index=i; }
        }
    }
    Ok(values)
}
