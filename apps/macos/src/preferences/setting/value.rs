/// 控件当前的值：勾选框是布尔，弹出菜单是选项下标，文本框是文本。
#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    /// 勾选框。
    Bool(bool),

    /// 弹出菜单选中项的下标。
    Index(usize),

    /// 文本框内容（未裁剪）。
    Text(String),

    /// 取色器当前的颜色（`NSColorWell`）。
    Color(objc2::rc::Retained<objc2_app_kit::NSColor>),
}
