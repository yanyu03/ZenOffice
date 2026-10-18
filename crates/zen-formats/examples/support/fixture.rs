use zen_engine::{
    calc::CellCoord,
    sheet::{
        Axis, CellFont, CellRange, HorizontalAlign, NumberFormat, PicturePlacement, Sheet,
        VerticalAlign,
    },
};

pub fn sample_workbook() -> Vec<Sheet> {
    let mut sheet = Sheet::new("销售演示", 24, 8);
    let entries = [
        ("A1", "Zen Office · OOXML 输出示例"),
        ("A3", "产品名称"),
        ("B3", "单价（元）"),
        ("C3", "数量"),
        ("D3", "金额（元）"),
        ("A4", "键盘"),
        ("B4", "128"),
        ("C4", "50"),
        ("D4", "=B4*C4"),
        ("A5", "鼠标"),
        ("B5", "256"),
        ("C5", "30"),
        ("D5", "=B5*C5"),
        ("A6", "显示器"),
        ("B6", "512"),
        ("C6", "15"),
        ("D6", "=B6*C6"),
        ("A7", "合计"),
        ("D7", "=SUM(D4:D6)"),
        ("A9", "折扣比例"),
        ("B9", "0.125"),
        ("A11", "导出来自 Rust 格式层，数据为固定演示数据。"),
        ("A15", "隐藏行测试内容"),
    ];
    sheet
        .set_cells_bulk(
            &entries
                .iter()
                .map(|(a, v)| (CellCoord::from_a1(a).unwrap(), v.to_string()))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    sheet
        .merge_cells(
            CellRange::new(CellCoord::new(0, 0), CellCoord::new(3, 0)),
            false,
        )
        .unwrap();
    sheet
        .merge_cells(
            CellRange::new(CellCoord::new(0, 10), CellCoord::new(4, 10)),
            false,
        )
        .unwrap();
    sheet.update_styles(&[CellCoord::new(0, 0)], |s| {
        s.bold = true;
        s.font_size = 20;
        s.text_color = [18, 94, 71];
        s.vertical = VerticalAlign::Center;
    });
    for row in 2..=6 {
        let coords: Vec<_> = (0..4).map(|col| CellCoord::new(col, row)).collect();
        sheet.update_styles(&coords, |s| {
            s.borders = 15;
            s.vertical = VerticalAlign::Center;
            if row == 2 {
                s.bold = true;
                s.fill_color = Some([22, 120, 89]);
                s.text_color = [255, 255, 255];
                s.alignment_set = true;
                s.horizontal = HorizontalAlign::Center;
            } else if row == 6 {
                s.bold = true;
                s.fill_color = Some([221, 239, 230]);
            } else if row % 2 == 1 {
                s.fill_color = Some([244, 248, 246]);
            }
        });
        if row > 2 {
            for col in [1, 3] {
                sheet.set_format(CellCoord::new(col, row), NumberFormat::Currency);
            }
        }
    }
    sheet.set_format(CellCoord::new(1, 8), NumberFormat::Percentage);
    sheet
        .set_hyperlink(
            CellCoord::new(0, 11),
            "OOXML 标准资料",
            "https://dev.ecma-international.org/publications-and-standards/standards/ecma-376/",
        )
        .unwrap();
    sheet
        .set_hyperlink(CellCoord::new(0, 12), "跳转到键盘行", "#A4")
        .unwrap();
    sheet.set_axis_size(Axis::Column, 0, Some(240.0));
    for col in 1..=3 {
        sheet.set_axis_size(Axis::Column, col, Some(125.0));
    }
    sheet.set_axis_size(Axis::Row, 0, Some(48.0));
    sheet.set_axis_size(Axis::Row, 2, Some(36.0));
    sheet.set_axis_hidden(Axis::Row, 14, 14, true).unwrap();
    sheet.set_axis_hidden(Axis::Column, 7, 7, true).unwrap();
    let mut pixels = Vec::new();
    for y in 0..60 {
        for x in 0..120 {
            let color = if (x / 20 + y / 20) % 2 == 0 {
                [27, 136, 102, 255]
            } else {
                [223, 242, 234, 255]
            };
            pixels.extend_from_slice(&color);
        }
    }
    sheet
        .insert_picture(
            "Rust RGBA 图片测试".into(),
            pixels,
            [120, 60],
            PicturePlacement {
                anchor: CellCoord::new(5, 2),
                offset: [8.0, 6.0],
                size: [120.0, 60.0],
            },
        )
        .unwrap();
    let mut text = Sheet::new("文本边界", 12, 4);
    text.set_cells_bulk(&[
        (CellCoord::new(0, 0), "兼容格式字段测试".into()),
        (CellCoord::new(0, 2), "前后空格".into()),
        (CellCoord::new(1, 2), "  中文保留空格  ".into()),
        (CellCoord::new(0, 3), "逗号和引号".into()),
        (CellCoord::new(1, 3), "中文,逗号和\"引号\"".into()),
        (CellCoord::new(0, 4), "单元格内换行".into()),
        (CellCoord::new(1, 4), "第一行\r\n第二行".into()),
        (CellCoord::new(0, 5), "数值精度".into()),
        (CellCoord::new(1, 5), "1.23456789".into()),
        (CellCoord::new(0, 6), "空格工作表链接".into()),
    ])
    .unwrap();
    text.set_axis_size(Axis::Column, 0, Some(210.0));
    text.set_axis_size(Axis::Column, 1, Some(290.0));
    text.update_styles(&[CellCoord::new(1, 4)], |s| {
        s.wrap = true;
        s.italic = true;
        s.font = CellFont::TimesNewRoman;
        s.font_size = 14;
    });
    text.set_axis_size(Axis::Row, 4, Some(56.0));
    text.update_styles(&[CellCoord::new(0, 0)], |s| {
        s.bold = true;
        s.font_size = 18;
        s.text_color = [18, 94, 71];
    });
    vec![sheet, text]
}
