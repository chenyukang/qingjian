//! 内容的垂直排布方向。默认从上往下；候选窗贴到光标上方时整块倒过来，让首选紧贴光标。

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VerticalOrder {
    /// 拼音行在上、候选往下排、页脚在最下（候选窗在光标下方）。
    #[default]
    TopDown,

    /// 整块垂直镜像：页脚在最上、候选从下往上排、拼音行落到最下（候选窗在光标上方）。
    BottomUp,
}
