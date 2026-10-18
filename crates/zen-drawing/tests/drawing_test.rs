use zen_drawing::*;

#[test]
fn test_drawingml_serialization() {
    let mut shape = Shape::rounded_rectangle(
        101,
        EMU_PER_INCH,
        EMU_PER_INCH,
        EMU_PER_INCH * 4,
        EMU_PER_INCH * 2,
        Color::hex("1FBB7D"),
    );
    shape.stroke = Some(Stroke {
        color: Color::hex("FFFFFF"),
        width_emu: 25_400, // 2pt
        dashed: false,
    });

    let mut tb = TextBody::default();
    let mut p = scene::TextParagraph::default();
    p.align = scene::TextAlign::Center;
    p.runs.push(scene::TextRun {
        text: "Zen Drawing Unified Graphics".into(),
        font_size_hundredth_pt: 2400, // 24pt
        bold: true,
        italic: false,
        color: Some(Color::hex("FFFFFF")),
    });
    tb.paragraphs.push(p);
    shape.text_body = Some(tb);

    // 1. 测试 PresentationML 渲染
    let xml = DrawingMLWriter::write_shape_element(&shape, "p");
    assert!(xml.contains("<p:sp>"));
    assert!(xml.contains("roundRect"));
    assert!(xml.contains("<a:srgbClr val=\"1FBB7D\"/>"));
    assert!(xml.contains("Zen Drawing Unified Graphics"));

    // 2. 测试 SVG 导出
    let svg = SvgExporter::export_svg(&[shape], EMU_PER_INCH * 6, EMU_PER_INCH * 4);
    assert!(svg.contains("<svg"));
    assert!(svg.contains("<rect"));
    assert!(svg.contains("fill=\"#1FBB7D\""));
    assert!(svg.contains("Zen Drawing Unified Graphics"));
}

#[test]
fn test_custom_path_drawingml_and_svg() {
    let mut shape = Shape::rounded_rectangle(
        202,
        0,
        0,
        100_000,
        100_000,
        Color::hex("FF5500"),
    );
    shape.geometry = Geometry::CustomPath("M 0 0 L 100 0 L 100 100 Z".to_string());

    // 验证 DrawingML 输出包含真实的 a:custGeom 以及 moveTo / lnTo / close 节点，绝非空标签
    let xml = DrawingMLWriter::write_shape_element(&shape, "p");
    assert!(xml.contains("<a:custGeom>"), "Must generate <a:custGeom>");
    assert!(xml.contains("<a:moveTo><a:pt x=\"0\" y=\"0\"/></a:moveTo>"), "Must have moveTo");
    assert!(xml.contains("<a:lnTo><a:pt x=\"100\" y=\"0\"/></a:lnTo>"), "Must have lnTo");
    assert!(xml.contains("<a:close/>"), "Must have close");

    // 验证 SVG 导出生成真实的 <path d="..." />
    let svg = SvgExporter::export_svg(&[shape], 200_000, 200_000);
    assert!(svg.contains("<path d=\"M 0 0 L 100 0 L 100 100 Z\""));
}

#[test]
fn test_svg_path_multi_coordinates_and_syntax_rejection() {
    // 1. 测试 SVG 规范规定的 M/L 多组参数连续解释
    let multi_arg_path = "M 0 0 10 10 20 20 L 30 30 40 40 H 50 60 V 70 80 Z";
    let res = writer::svg_path_to_drawingml_path(multi_arg_path, 100, 100).expect("Multi-arg path parse");
    // 首对生成 moveTo，后续两对生成 lnTo
    assert!(res.contains("<a:moveTo><a:pt x=\"0\" y=\"0\"/></a:moveTo>"));
    assert!(res.contains("<a:lnTo><a:pt x=\"10\" y=\"10\"/></a:lnTo>"));
    assert!(res.contains("<a:lnTo><a:pt x=\"20\" y=\"20\"/></a:lnTo>"));
    // L 后续成对解释
    assert!(res.contains("<a:lnTo><a:pt x=\"30\" y=\"30\"/></a:lnTo>"));
    assert!(res.contains("<a:lnTo><a:pt x=\"40\" y=\"40\"/></a:lnTo>"));
    // H/V 连续解释
    assert!(res.contains("<a:lnTo><a:pt x=\"50\" y=\"40\"/></a:lnTo>"));
    assert!(res.contains("<a:lnTo><a:pt x=\"60\" y=\"40\"/></a:lnTo>"));
    assert!(res.contains("<a:lnTo><a:pt x=\"60\" y=\"70\"/></a:lnTo>"));
    assert!(res.contains("<a:lnTo><a:pt x=\"60\" y=\"80\"/></a:lnTo>"));
    assert!(res.contains("<a:close/>"));

    // 2. 测试不受支持或非法指令显式拒绝，绝不静默使用矩形伪装
    let unsupported = "M 0 0 X 100 100 Z";
    let err = writer::svg_path_to_drawingml_path(unsupported, 100, 100).unwrap_err();
    match err {
        Error::UnsupportedGeometry(msg) => assert!(msg.contains("暂不支持的 SVG 指令: 'X'"), "{msg}"),
        other => panic!("Expected UnsupportedGeometry, got: {other:?}"),
    }

    // 3. 测试坐标参数不完整报错
    let incomplete = "M 0";
    let err2 = writer::svg_path_to_drawingml_path(incomplete, 100, 100).unwrap_err();
    match err2 {
        Error::InvalidGeometry(msg) => assert!(msg.contains("缺少 Y 坐标"), "{msg}"),
        other => panic!("Expected InvalidGeometry, got: {other:?}"),
    }

    // 4. 空路径显式报错
    let empty_err = writer::svg_path_to_drawingml_path("", 100, 100).unwrap_err();
    match empty_err {
        Error::InvalidGeometry(msg) => assert!(msg.contains("数据为空"), "{msg}"),
        other => panic!("Expected InvalidGeometry, got: {other:?}"),
    }
}

#[test]
fn test_svg_path_comprehensive_spec_conformance() {
    // 1. 相对命令与连续相对位移计算
    let rel_path = "m 10 20 l 5 5 h 10 v -5 z";
    let res1 = writer::svg_path_to_drawingml_path(rel_path, 100, 100).expect("Relative path");
    assert!(res1.contains("<a:moveTo><a:pt x=\"10\" y=\"20\"/></a:moveTo>"));
    assert!(res1.contains("<a:lnTo><a:pt x=\"15\" y=\"25\"/></a:lnTo>"));
    assert!(res1.contains("<a:lnTo><a:pt x=\"25\" y=\"25\"/></a:lnTo>"));
    assert!(res1.contains("<a:lnTo><a:pt x=\"25\" y=\"20\"/></a:lnTo>"));
    assert!(res1.contains("<a:close/>"));

    // 2. 闭合后继续绘制多个子路径 (Z 后接新的 M/L)
    let multi_subpath = "M 0 0 L 10 10 Z M 50 50 L 60 60 Z";
    let res2 = writer::svg_path_to_drawingml_path(multi_subpath, 100, 100).expect("Multi subpath");
    assert!(res2.contains("<a:moveTo><a:pt x=\"0\" y=\"0\"/></a:moveTo>"));
    assert!(res2.contains("<a:lnTo><a:pt x=\"10\" y=\"10\"/></a:lnTo>"));
    assert!(res2.contains("<a:close/>"));
    assert!(res2.contains("<a:moveTo><a:pt x=\"50\" y=\"50\"/></a:moveTo>"));
    assert!(res2.contains("<a:lnTo><a:pt x=\"60\" y=\"60\"/></a:lnTo>"));

    // 3. 科学计数法格式 (1e2, 2.5e1, -1e-1, 1E+2)
    let sci_path = "M 1e2 2.5e1 L 3e1 1E+2";
    let res3 = writer::svg_path_to_drawingml_path(sci_path, 100, 100).expect("Scientific notation");
    assert!(res3.contains("<a:moveTo><a:pt x=\"100\" y=\"25\"/></a:moveTo>"));
    assert!(res3.contains("<a:lnTo><a:pt x=\"30\" y=\"100\"/></a:lnTo>"));

    // 4. 紧凑无空格负号分隔符 (M10-20L-30-40)
    let compact_path = "M10-20L-30-40";
    let res4 = writer::svg_path_to_drawingml_path(compact_path, 100, 100).expect("Compact signs");
    assert!(res4.contains("<a:moveTo><a:pt x=\"10\" y=\"-20\"/></a:moveTo>"));
    assert!(res4.contains("<a:lnTo><a:pt x=\"-30\" y=\"-40\"/></a:lnTo>"));

    // 5. 畸形 token 报错拒绝
    assert!(writer::svg_path_to_drawingml_path("M 10 abc", 100, 100).is_err());
    assert!(writer::svg_path_to_drawingml_path("M 10 20 L 30", 100, 100).is_err());
    assert!(writer::svg_path_to_drawingml_path("M 1e", 100, 100).is_err());
    assert!(writer::svg_path_to_drawingml_path("M 10 20 Q 30 40 50 60", 100, 100).is_err()); // Q 为暂不支持指令
}

