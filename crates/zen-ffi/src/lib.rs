//! # Zen Office 通用外部 C-ABI / FFI 协议标准接口 (zen-ffi)
//!
//! 提供跨语言、无运行时依赖的 Windows 标准 C-ABI 动态链接库接口，方便外部自动化测试、
//! Python、C/C++、C# 及 CI/CD 物理探针直接加载与调用。
//!
//! ## 错误码与协议约定 (HRESULT 规范)
//! - `0` (`ZEN_OK`): 操作成功
//! - `-1` (`ZEN_ERR_NULL_PTR`): 空指针参数
//! - `-2` (`ZEN_ERR_UTF8`): 非法 UTF-8 编码文本
//! - `-3` (`ZEN_ERR_INVALID_ARG`): 参数越界或无效
//! - `-4` (`ZEN_ERR_ENGINE_FAULT`): 引擎内部求值或排版异常
//! - `-5` (`ZEN_ERR_BUFFER_TOO_SMALL`): 接收缓冲区分离长度不足

#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::CStr;
use std::os::raw::{c_char, c_double, c_int, c_uint};
use std::ptr;

use zen_drawing::{Color, Shape, SvgExporter};
use zen_engine::calc::{CellCoord, CellValue, FormulaEngine};
use zen_engine::sheet::Sheet;

pub const ZEN_OK: c_int = 0;
pub const ZEN_ERR_NULL_PTR: c_int = -1;
pub const ZEN_ERR_UTF8: c_int = -2;
pub const ZEN_ERR_INVALID_ARG: c_int = -3;
pub const ZEN_ERR_ENGINE_FAULT: c_int = -4;
pub const ZEN_ERR_BUFFER_TOO_SMALL: c_int = -5;

// ============================================================================
// 1. 引擎基础与版本协议 (Engine Base & Protocol Version)
// ============================================================================

/// 获取 Zen 引擎 C-ABI 协议版本号 (例如 0x00010000 = 1.0.0)
#[unsafe(no_mangle)]
pub extern "C" fn zen_engine_version() -> c_uint {
    0x00010000
}

/// 检查引擎健康状态，返回协议就绪字符串
#[unsafe(no_mangle)]
pub extern "C" fn zen_engine_health_probe(out_buf: *mut c_char, buf_len: usize) -> c_int {
    if out_buf.is_null() || buf_len == 0 {
        return ZEN_ERR_NULL_PTR;
    }
    let msg = "ZEN_OFFICE_ENGINE_CORE_READY:Windows_NT_C_ABI_v1.0";
    copy_string_to_c_buf(msg, out_buf, buf_len)
}

// ============================================================================
// 2. 表格与公式无状态计算接口 (Stateless Formula & Sheet Evaluation API)
// ============================================================================

/// 独立单公式动态求值接口 (纯计算、无副作用，返回浮点值)
/// 示例: formula = "=SUM(10, 20) * 1.5 - (5 + 3)" -> *out_result = 37.0
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zen_calc_eval_formula_f64(
    formula: *const c_char,
    out_result: *mut c_double,
) -> c_int {
    if formula.is_null() || out_result.is_null() {
        return ZEN_ERR_NULL_PTR;
    }
    let c_str = match CStr::from_ptr(formula).to_str() {
        Ok(s) => s,
        Err(_) => return ZEN_ERR_UTF8,
    };

    let dummy_resolver = |_sheet: Option<&str>, _coord: CellCoord| -> CellValue {
        CellValue::Empty
    };
    let engine = FormulaEngine::with_resolver(&dummy_resolver);
    let val = engine.eval(c_str);

    match val {
        CellValue::Number(n) => {
            *out_result = n;
            ZEN_OK
        }
        CellValue::Error(_) => ZEN_ERR_ENGINE_FAULT,
        _ => ZEN_ERR_INVALID_ARG,
    }
}

/// 内存工作表不透明句柄 (Opaque Pointer)
pub struct ZenSheetHandle(Box<Sheet>);

/// 创建一个空内存工作表 (指定行数与列数)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zen_sheet_create(
    name: *const c_char,
    rows: usize,
    cols: usize,
) -> *mut ZenSheetHandle {
    let sheet_name = if name.is_null() {
        "Sheet1"
    } else {
        CStr::from_ptr(name).to_str().unwrap_or("Sheet1")
    };
    let sheet = Box::new(Sheet::new(sheet_name, rows, cols));
    Box::into_raw(Box::new(ZenSheetHandle(sheet)))
}

/// 销毁并释放工作表句柄
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zen_sheet_free(handle: *mut ZenSheetHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// 向工作表写入单元格内容 (A1 格式，支持字面量或公式)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zen_sheet_set_cell_a1(
    handle: *mut ZenSheetHandle,
    a1_coord: *const c_char,
    text: *const c_char,
) -> c_int {
    if handle.is_null() || a1_coord.is_null() || text.is_null() {
        return ZEN_ERR_NULL_PTR;
    }
    let a1_str = match CStr::from_ptr(a1_coord).to_str() {
        Ok(s) => s,
        Err(_) => return ZEN_ERR_UTF8,
    };
    let text_str = match CStr::from_ptr(text).to_str() {
        Ok(s) => s,
        Err(_) => return ZEN_ERR_UTF8,
    };

    let coord = match CellCoord::from_a1(a1_str) {
        Some(c) => c,
        None => return ZEN_ERR_INVALID_ARG,
    };

    let sheet = &mut (*handle).0;
    if !sheet.set_cell(coord, text_str) {
        return ZEN_ERR_INVALID_ARG;
    }
    ZEN_OK
}

/// 触发工作表整表重算
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zen_sheet_recalculate(handle: *mut ZenSheetHandle) -> c_int {
    if handle.is_null() {
        return ZEN_ERR_NULL_PTR;
    }
    let sheet = &mut (*handle).0;
    sheet.recalculate();
    ZEN_OK
}

/// 读取单元格求值后的数值 (f64)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zen_sheet_get_value_f64(
    handle: *mut ZenSheetHandle,
    a1_coord: *const c_char,
    out_val: *mut c_double,
) -> c_int {
    if handle.is_null() || a1_coord.is_null() || out_val.is_null() {
        return ZEN_ERR_NULL_PTR;
    }
    let a1_str = match CStr::from_ptr(a1_coord).to_str() {
        Ok(s) => s,
        Err(_) => return ZEN_ERR_UTF8,
    };
    let coord = match CellCoord::from_a1(a1_str) {
        Some(c) => c,
        None => return ZEN_ERR_INVALID_ARG,
    };

    let sheet = &(*handle).0;
    if let Some(val) = sheet.get_value(coord).as_f64() {
        *out_val = val;
        ZEN_OK
    } else {
        ZEN_ERR_INVALID_ARG
    }
}

/// 读取单元格格式化显示文本 (如 ¥128,000.50 或 28.1%)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zen_sheet_get_formatted_text(
    handle: *mut ZenSheetHandle,
    a1_coord: *const c_char,
    out_buf: *mut c_char,
    buf_len: usize,
) -> c_int {
    if handle.is_null() || a1_coord.is_null() || out_buf.is_null() || buf_len == 0 {
        return ZEN_ERR_NULL_PTR;
    }
    let a1_str = match CStr::from_ptr(a1_coord).to_str() {
        Ok(s) => s,
        Err(_) => return ZEN_ERR_UTF8,
    };
    let coord = match CellCoord::from_a1(a1_str) {
        Some(c) => c,
        None => return ZEN_ERR_INVALID_ARG,
    };

    let sheet = &(*handle).0;
    let disp = sheet.get_formatted_display(coord);
    copy_string_to_c_buf(&disp, out_buf, buf_len)
}

// ============================================================================
// 3. 统一图形中台与 DrawingML / SVG 渲染测试接口 (Drawing & Graphics API)
// ============================================================================

/// 将简单矩形卡片直接渲染并写出为 W3C 标准 SVG 字符串 (对标 MSOSVG.DLL 外部探测通道)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zen_drawing_card_to_svg(
    title: *const c_char,
    fill_hex: *const c_char,
    width_pt: c_double,
    height_pt: c_double,
    out_buf: *mut c_char,
    buf_len: usize,
) -> c_int {
    if title.is_null() || fill_hex.is_null() || out_buf.is_null() || buf_len == 0 {
        return ZEN_ERR_NULL_PTR;
    }
    let title_str = match CStr::from_ptr(title).to_str() {
        Ok(s) => s,
        Err(_) => return ZEN_ERR_UTF8,
    };
    let hex_str = match CStr::from_ptr(fill_hex).to_str() {
        Ok(s) => s,
        Err(_) => return ZEN_ERR_UTF8,
    };

    if width_pt <= 0.0 || height_pt <= 0.0 || !width_pt.is_finite() || !height_pt.is_finite() {
        return ZEN_ERR_INVALID_ARG;
    }

    let color = match Color::from_hex(hex_str) {
        Ok(c) => c,
        Err(_) => return ZEN_ERR_INVALID_ARG,
    };

    let width_emu = (width_pt * 12_700.0) as i64;
    let height_emu = (height_pt * 12_700.0) as i64;

    let mut shape = Shape::rounded_rectangle(
        1,
        12_700 * 10,
        12_700 * 10,
        width_emu,
        height_emu,
        color,
    );
    let mut tb = zen_drawing::TextBody::default();
    let mut p = zen_drawing::scene::TextParagraph::default();
    p.align = zen_drawing::scene::TextAlign::Center;
    p.runs.push(zen_drawing::scene::TextRun {
        text: title_str.to_string(),
        font_size_hundredth_pt: 1600,
        bold: true,
        italic: false,
        color: Some(Color::hex("FFFFFF")),
    });
    tb.paragraphs.push(p);
    shape.text_body = Some(tb);

    let svg = SvgExporter::export_svg(&[shape], width_emu + 12_700 * 20, height_emu + 12_700 * 20);
    copy_string_to_c_buf(&svg, out_buf, buf_len)
}

// ============================================================================
// 内部字符串安全拷贝辅助函数 (遵循标准 C NUL 终止与边界截断保护)
// ============================================================================

fn copy_string_to_c_buf(src: &str, dest: *mut c_char, dest_len: usize) -> c_int {
    if dest_len == 0 {
        return ZEN_ERR_BUFFER_TOO_SMALL;
    }
    let bytes = src.as_bytes();
    if bytes.len() >= dest_len {
        return ZEN_ERR_BUFFER_TOO_SMALL;
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, dest, bytes.len());
        *dest.add(bytes.len()) = 0; // NUL 终止符
    }
    ZEN_OK
}
