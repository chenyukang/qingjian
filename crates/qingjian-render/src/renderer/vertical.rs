//! 竖排：一行一个候选，序号 / 候选词 / 译文三列，页码在右下角。

use super::columns::Columns;
use super::{Metrics, Renderer};
use crate::canvas::Canvas;
use crate::frame::{Frame, Row};
use crate::vertical_order::VerticalOrder;

impl Renderer {
    pub(super) fn vertical_size(&mut self, frame: &Frame, m: &Metrics) -> (f32, f32) {
        let columns = self.columns(&frame.rows, m);
        let mut width = columns.index_width + m.column_gap() + columns.text_width;
        if columns.annotation_width > 0.0 {
            width += m.column_gap() + columns.annotation_width;
        }
        let mut height = columns.row_height * frame.rows.len() as f32;
        if let Some(footer) = frame.footer.as_deref() {
            let footer_size = self.measure(footer, &m.index_style());
            width = width.max(footer_size.width);
            height += footer_size.height + m.row_padding();
        }
        (width, height)
    }

    fn columns(&mut self, rows: &[Row], m: &Metrics) -> Columns {
        let mut columns = Columns {
            index_width: 0.0,
            text_width: 0.0,
            annotation_width: 0.0,
            row_height: 0.0,
        };
        let text_style = m.text_style();
        let index_style = m.index_style();
        let annotation_style = m.annotation_style(m.theme.colors.gloss);
        for row in rows {
            let index = self.measure(&row.index, &index_style);
            // 长文本（翻译 / 纠错的结果）按固定最大宽度折行：宽度封顶、行高按行数算
            let wrapped = self.wrap_row_text(&row.text, &text_style, m);
            let mut text_width = wrapped.width;
            if row.cloud {
                text_width += m.cloud_width();
            }
            text_width += self.code_width(row, m);
            let annotation: f32 = row
                .annotation
                .iter()
                .map(|(s, _)| self.measure(s, &annotation_style).width)
                .sum();
            columns.index_width = columns.index_width.max(index.width);
            columns.text_width = columns.text_width.max(text_width);
            columns.annotation_width = columns.annotation_width.max(annotation);
            columns.row_height = columns
                .row_height
                .max(wrapped.height + m.row_padding() * 2.0);
        }
        columns
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_vertical(
        &mut self,
        canvas: &mut Canvas,
        frame: &Frame,
        m: &Metrics,
        left: f32,
        mut y: f32,
        content_width: f32,
        order: VerticalOrder,
    ) {
        // 量尺寸时已整形过一遍，这里再整形一遍；等渲染器定型再把结果从 render 传下来。
        let columns = self.columns(&frame.rows, m);
        let text_x = left + m.padding() + columns.index_width + m.column_gap();
        let annotation_x = text_x + columns.text_width + m.column_gap();
        let text_height = m.px(m.theme.text_font.line_height);
        // BottomUp（候选窗在光标上方）时页码先占最上面，候选行再倒着走：末位排在最上、首选落到最下贴着光标
        let rows = frame.rows.len();
        if order == VerticalOrder::BottomUp {
            y += self.draw_footer(canvas, frame, m, left, y, content_width);
        }
        for step in 0..rows {
            let i = match order {
                VerticalOrder::TopDown => step,
                VerticalOrder::BottomUp => rows - 1 - step,
            };
            let row = &frame.rows[i];
            if Some(i) == frame.highlighted {
                self.fill_highlight(
                    canvas,
                    m,
                    left + m.padding() / 2.0,
                    y,
                    content_width - m.padding(),
                    columns.row_height,
                );
            }
            let top = y + m.row_padding();
            let small_offset = m.small_offset(text_height);
            self.draw_text(
                canvas,
                &row.index,
                &m.index_style(),
                left + m.padding(),
                top + small_offset,
            );
            self.draw_word(canvas, m, row, text_x, top, text_height);
            let mut x = annotation_x;
            for (segment, tone) in &row.annotation {
                let style = m.annotation_style(m.tone_color(*tone));
                x += self.draw_text(canvas, segment, &style, x, top + small_offset);
            }
            y += columns.row_height;
        }
        if order == VerticalOrder::TopDown {
            self.draw_footer(canvas, frame, m, left, y, content_width);
        }
    }

    /// 页码：靠右一行（BottomUp 时它排到候选体最上面）。返回它占的高度，含它下面的行距。
    fn draw_footer(
        &mut self,
        canvas: &mut Canvas,
        frame: &Frame,
        m: &Metrics,
        left: f32,
        y: f32,
        content_width: f32,
    ) -> f32 {
        let Some(footer) = frame.footer.as_deref() else {
            return 0.0;
        };
        let style = m.index_style();
        let size = self.measure(footer, &style);
        self.draw_text(
            canvas,
            footer,
            &style,
            left + content_width - m.padding() - size.width,
            y + m.row_padding(),
        );
        size.height + m.row_padding()
    }
}
