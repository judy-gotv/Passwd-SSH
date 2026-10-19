//! [`Terminal`]: VT/xterm emulation state built on `alacritty_terminal`.
//!
//! Feeding bytes in, reading a styled render model out. The UI layer
//! (egui) draws the render model; it never touches `alacritty_terminal`
//! directly.
//!
//! Supported (docs §6.4): UTF-8, CJK wide chars, Emoji, ANSI 16/256/true
//! color, cursor styles, selection/copy helpers, scrollback, search, resize.
//!
//! Author: Passwd Word

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor, Rgb};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Terminal errors.
#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("terminal is closed")]
    Closed,
}

/// Plain RGB color, UI-toolkit independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Pack as 0xRRGGBB.
    pub fn to_u32(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }
}

impl From<Rgb> for RgbColor {
    fn from(c: Rgb) -> Self {
        Self::new(c.r, c.g, c.b)
    }
}

/// One run of identically-styled text on a rendered line.
#[derive(Debug, Clone)]
pub struct StyledSpan {
    pub text: String,
    /// Column (in cells) where this span starts, for hit-testing.
    pub start_col: usize,
    pub fg: RgbColor,
    pub bg: RgbColor,
    pub bold: bool,
    pub underline: bool,
}

/// One visible terminal row.
#[derive(Debug, Clone, Default)]
pub struct RenderLine {
    pub spans: Vec<StyledSpan>,
}

impl RenderLine {
    /// Plain text of the line (trailing whitespace trimmed).
    pub fn text(&self) -> String {
        let s: String = self.spans.iter().map(|sp| sp.text.as_str()).collect();
        s.trim_end().to_string()
    }
}

/// The visible terminal frame, ready for the UI to draw.
#[derive(Debug, Clone)]
pub struct RenderedTerminal {
    pub lines: Vec<RenderLine>,
    /// Cursor position in (row, col) screen coordinates.
    pub cursor: (usize, usize),
    pub cursor_visible: bool,
}

struct TermDimensions {
    cols: usize,
    rows: usize,
}

impl Dimensions for TermDimensions {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Terminal emulation state.
pub struct Terminal {
    term: Term<VoidListener>,
    processor: Processor,
}

impl Terminal {
    /// Create a terminal with the given size (in cells).
    pub fn new(cols: usize, rows: usize) -> Self {
        let config = Config {
            scrolling_history: 10_000,
            ..Default::default()
        };
        let dims = TermDimensions {
            cols: cols.max(1),
            rows: rows.max(1),
        };
        let term = Term::new(config, &dims, VoidListener);
        Self {
            term,
            processor: Processor::new(),
        }
    }

    /// Feed raw PTY/SSH output bytes into the emulator.
    pub fn feed(&mut self, bytes: &[u8]) {
        // `Term` implements `vte::ansi::Handler`.
        self.processor.advance(&mut self.term, bytes);
    }

    /// Resize the terminal grid.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let dims = TermDimensions {
            cols: cols.max(1),
            rows: rows.max(1),
        };
        self.term.resize(dims);
    }

    /// Scroll the viewport; positive = up (toward older history).
    pub fn scroll(&mut self, delta: i32) {
        self.term.scroll_display(Scroll::Delta(delta));
    }

    /// Scroll to the very bottom (follow new output).
    pub fn scroll_to_bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }

    /// Whether the viewport is currently scrolled up into history.
    pub fn is_scrolled_up(&self) -> bool {
        self.term.grid().display_offset() > 0
    }

    /// Scroll so that the given grid line (0 = topmost, as returned by
    /// [`Terminal::search`]) becomes visible near the bottom of the viewport.
    pub fn scroll_to_grid_line(&mut self, line: usize) {
        let grid = self.term.grid();
        let bottom = grid.bottommost_line().0.max(0) as usize;
        let target = bottom.saturating_sub(line);
        let current = grid.display_offset();
        let delta = target as i32 - current as i32;
        if delta != 0 {
            self.term.scroll_display(Scroll::Delta(delta));
        }
    }

    /// Current grid size in (cols, rows).
    pub fn size(&self) -> (usize, usize) {
        (self.term.columns(), self.term.screen_lines())
    }

    /// Render the visible frame with theme default colors.
    pub fn render(&self, default_fg: RgbColor, default_bg: RgbColor) -> RenderedTerminal {
        let content = self.term.renderable_content();
        let cols = self.term.columns();
        let mut lines: Vec<RenderLine> = Vec::with_capacity(self.term.screen_lines());
        let mut cur_line = RenderLine::default();
        let mut cur_span: Option<StyledSpan> = None;
        let mut col: usize = 0;

        // `display_iter` yields cells top-left → bottom-right of the viewport.
        for indexed in content.display_iter {
            let cell = indexed.cell;
            // Wide-char spacers occupy a cell but render nothing.
            let is_spacer = cell
                .flags
                .contains(alacritty_terminal::term::cell::Flags::WIDE_CHAR_SPACER);

            if !is_spacer {
                let (mut fg, mut bg) =
                    resolve_cell_colors(&cell.fg, &cell.bg, content.colors, default_fg, default_bg);
                if cell
                    .flags
                    .contains(alacritty_terminal::term::cell::Flags::INVERSE)
                {
                    std::mem::swap(&mut fg, &mut bg);
                }
                let bold = cell
                    .flags
                    .contains(alacritty_terminal::term::cell::Flags::BOLD);
                let underline = cell
                    .flags
                    .contains(alacritty_terminal::term::cell::Flags::UNDERLINE);
                let same_style = cur_span.as_ref().is_some_and(|s| {
                    s.fg == fg && s.bg == bg && s.bold == bold && s.underline == underline
                });
                if same_style {
                    cur_span.as_mut().unwrap().text.push(cell.c);
                } else {
                    if let Some(span) = cur_span.take() {
                        cur_line.spans.push(span);
                    }
                    cur_span = Some(StyledSpan {
                        text: String::from(cell.c),
                        start_col: col,
                        fg,
                        bg,
                        bold,
                        underline,
                    });
                }
            }

            col += 1;
            if col >= cols {
                if let Some(span) = cur_span.take() {
                    cur_line.spans.push(span);
                }
                lines.push(std::mem::take(&mut cur_line));
                col = 0;
            }
        }
        if col > 0 || cur_span.is_some() {
            if let Some(span) = cur_span.take() {
                cur_line.spans.push(span);
            }
            lines.push(cur_line);
        }

        let offset = self.term.grid().display_offset() as i32;
        let cursor = content.cursor;
        let cursor_row = cursor.point.line.0 + offset;
        let cursor_visible = !matches!(cursor.shape, CursorShape::Hidden)
            && cursor_row >= 0
            && (cursor_row as usize) < lines.len();

        RenderedTerminal {
            lines,
            cursor: (cursor_row.max(0) as usize, cursor.point.column.0),
            cursor_visible,
        }
    }

    /// Plain text of every visible line, top → bottom.
    pub fn visible_text(&self) -> Vec<String> {
        self.render(RgbColor::new(255, 255, 255), RgbColor::new(0, 0, 0))
            .lines
            .iter()
            .map(RenderLine::text)
            .collect()
    }

    /// Plain text of the whole scrollback + screen, oldest → newest.
    pub fn full_text(&self) -> Vec<String> {
        use alacritty_terminal::index::{Column, Line};
        let grid = self.term.grid();
        let top = grid.topmost_line().0;
        let bottom = grid.bottommost_line().0;
        let cols = grid.columns();
        let mut out = Vec::new();
        for line_no in top..=bottom {
            let mut s = String::new();
            for col_no in 0..cols {
                let cell = &grid[Line(line_no)][Column(col_no)];
                if cell
                    .flags
                    .contains(alacritty_terminal::term::cell::Flags::WIDE_CHAR_SPACER)
                {
                    continue;
                }
                s.push(cell.c);
            }
            out.push(s.trim_end().to_string());
        }
        out
    }

    /// Search scrollback + screen for `query`.
    ///
    /// Returns `(line_index, column)` matches in [`Terminal::full_text`]
    /// coordinates, oldest → newest.
    pub fn search(&self, query: &str) -> Vec<(usize, usize)> {
        if query.is_empty() {
            return Vec::new();
        }
        let mut hits = Vec::new();
        for (i, line) in self.full_text().iter().enumerate() {
            let mut start = 0;
            while let Some(pos) = line[start..].find(query) {
                let col = start + pos;
                // Column in *cells*: approximate with char count (wide chars
                // occupy 2 cells; good enough for highlight positioning).
                hits.push((i, col));
                start = col + query.len().max(1);
            }
        }
        hits
    }

    /// Clear the scrollback history (keeps the visible screen).
    pub fn clear_scrollback(&mut self) {
        // ED with parameter 3 = clear scrollback (xterm).
        self.feed(b"\x1b[3J");
    }
}

// ---------------------------------------------------------------------------
// Color resolution
// ---------------------------------------------------------------------------

/// Standard xterm 16-color palette.
fn named_palette(nc: NamedColor) -> Option<RgbColor> {
    use NamedColor::*;
    let (r, g, b) = match nc {
        Black => (0, 0, 0),
        Red => (205, 0, 0),
        Green => (0, 205, 0),
        Yellow => (205, 205, 0),
        Blue => (0, 0, 238),
        Magenta => (205, 0, 205),
        Cyan => (0, 205, 205),
        White => (229, 229, 229),
        BrightBlack => (128, 128, 128),
        BrightRed => (255, 0, 0),
        BrightGreen => (0, 255, 0),
        BrightYellow => (255, 255, 0),
        BrightBlue => (92, 92, 255),
        BrightMagenta => (255, 0, 255),
        BrightCyan => (0, 255, 255),
        BrightWhite => (255, 255, 255),
        DimBlack => (0, 0, 0),
        DimRed => (128, 0, 0),
        DimGreen => (0, 128, 0),
        DimYellow => (128, 128, 0),
        DimBlue => (0, 0, 128),
        DimMagenta => (128, 0, 128),
        DimCyan => (0, 128, 128),
        DimWhite => (128, 128, 128),
        BrightForeground | DimForeground => (255, 255, 255),
        Foreground | Background | Cursor => return None,
    };
    Some(RgbColor::new(r, g, b))
}

/// xterm 256-color palette entry.
fn indexed_palette(i: u8) -> RgbColor {
    if i < 16 {
        let nc = match i {
            0 => NamedColor::Black,
            1 => NamedColor::Red,
            2 => NamedColor::Green,
            3 => NamedColor::Yellow,
            4 => NamedColor::Blue,
            5 => NamedColor::Magenta,
            6 => NamedColor::Cyan,
            7 => NamedColor::White,
            8 => NamedColor::BrightBlack,
            9 => NamedColor::BrightRed,
            10 => NamedColor::BrightGreen,
            11 => NamedColor::BrightYellow,
            12 => NamedColor::BrightBlue,
            13 => NamedColor::BrightMagenta,
            14 => NamedColor::BrightCyan,
            _ => NamedColor::BrightWhite,
        };
        return named_palette(nc).unwrap();
    }
    if i < 232 {
        let i = i - 16;
        let r = i / 36;
        let g = (i % 36) / 6;
        let b = i % 6;
        let conv = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
        return RgbColor::new(conv(r), conv(g), conv(b));
    }
    let v = 8 + (i - 232) * 10;
    RgbColor::new(v, v, v)
}

fn resolve_one(
    color: &Color,
    colors: &alacritty_terminal::term::color::Colors,
    is_fg: bool,
    default_fg: RgbColor,
    default_bg: RgbColor,
) -> RgbColor {
    match color {
        Color::Spec(rgb) => RgbColor::from(*rgb),
        Color::Indexed(i) => {
            if let Some(rgb) = colors[*i as usize] {
                RgbColor::from(rgb)
            } else {
                indexed_palette(*i)
            }
        }
        Color::Named(nc) => {
            if let Some(rgb) = colors[*nc] {
                return RgbColor::from(rgb);
            }
            match nc {
                NamedColor::Foreground => default_fg,
                NamedColor::Background => default_bg,
                NamedColor::Cursor => default_fg,
                _ => named_palette(*nc).unwrap_or(if is_fg { default_fg } else { default_bg }),
            }
        }
    }
}

fn resolve_cell_colors(
    fg: &Color,
    bg: &Color,
    colors: &alacritty_terminal::term::color::Colors,
    default_fg: RgbColor,
    default_bg: RgbColor,
) -> (RgbColor, RgbColor) {
    (
        resolve_one(fg, colors, true, default_fg, default_bg),
        resolve_one(bg, colors, false, default_fg, default_bg),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_term() -> Terminal {
        let mut t = Terminal::new(80, 24);
        t.feed(b"hello\r\nworld");
        t
    }

    #[test]
    fn basic_text_rendering() {
        let t = sample_term();
        let view = t.render(RgbColor::new(255, 255, 255), RgbColor::new(0, 0, 0));
        assert_eq!(view.lines.len(), 24);
        assert_eq!(view.lines[0].text(), "hello");
        assert_eq!(view.lines[1].text(), "world");
        assert_eq!(view.lines[2].text(), "");
    }

    #[test]
    fn ansi_colors_parse() {
        let mut t = Terminal::new(80, 24);
        t.feed(b"\x1b[31mred\x1b[0m plain");
        let view = t.render(RgbColor::new(255, 255, 255), RgbColor::new(0, 0, 0));
        let first = &view.lines[0].spans[0];
        assert_eq!(first.text, "red");
        assert_eq!((first.fg.r, first.fg.g, first.fg.b), (205, 0, 0));
    }

    #[test]
    fn truecolor_parse() {
        let mut t = Terminal::new(80, 24);
        t.feed(b"\x1b[38;2;10;20;30mX");
        let view = t.render(RgbColor::new(255, 255, 255), RgbColor::new(0, 0, 0));
        let first = &view.lines[0].spans[0];
        assert_eq!((first.fg.r, first.fg.g, first.fg.b), (10, 20, 30));
    }

    #[test]
    fn cjk_wide_chars() {
        let mut t = Terminal::new(80, 24);
        t.feed("你好".as_bytes());
        let view = t.render(RgbColor::new(255, 255, 255), RgbColor::new(0, 0, 0));
        assert_eq!(view.lines[0].text(), "你好");
    }

    #[test]
    fn search_finds_matches() {
        let t = sample_term();
        let hits = t.search("o");
        // "hello" has 1 'o' at col 4 (line 0), "world" has 1 at col 1 (line 1).
        assert_eq!(hits, vec![(0, 4), (1, 1)]);
        assert!(t.search("").is_empty());
        assert!(t.search("zzz").is_empty());
    }

    #[test]
    fn resize_keeps_content() {
        let mut t = sample_term();
        t.resize(100, 30);
        assert_eq!(t.size(), (100, 30));
        let view = t.render(RgbColor::new(255, 255, 255), RgbColor::new(0, 0, 0));
        assert_eq!(view.lines.len(), 30);
        assert_eq!(view.lines[0].text(), "hello");
    }

    #[test]
    fn cursor_tracking() {
        let t = sample_term();
        let view = t.render(RgbColor::new(255, 255, 255), RgbColor::new(0, 0, 0));
        // After "hello\r\nworld" the cursor sits at line 1, col 5.
        assert_eq!(view.cursor, (1, 5));
        assert!(view.cursor_visible);
    }

    #[test]
    fn indexed_palette_sanity() {
        assert_eq!(indexed_palette(0), RgbColor::new(0, 0, 0));
        assert_eq!(indexed_palette(15), RgbColor::new(255, 255, 255));
        assert_eq!(indexed_palette(16), RgbColor::new(0, 0, 0));
        assert_eq!(indexed_palette(231), RgbColor::new(255, 255, 255));
        let g = indexed_palette(240);
        assert_eq!((g.r, g.g, g.b), (88, 88, 88));
    }
}
