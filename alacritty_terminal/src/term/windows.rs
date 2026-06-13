use super::Term;
use crate::index::{Column, Line};
use crate::grid::Dimensions;

/// Powershell / conpty has a different resize behavior than Alacritty if line wraps occur
/// during resize:
/// - Alacritty keeps all blank lines in the bottom of the screen and any new line introduced
///   due to line wrapping is appended to the history section (consistent with iterm behavior).
/// - Conpty first consumes blank lines in the bottom of the screen and only appends to the
///   history if no more space is left in the bottom of the screen.
/// In order to keep Alacritty and conpty in sync after resize we need to adjust the Alacritty
/// state correspondingly. Otherwise bad things can happen because there is no additional state
/// sync happening between conpty and Alacritty.
pub fn adjust_to_conpty_resize_behavior<T>(term: &mut Term<T>, history_size_before_resize: usize) {
    let history_size_change = (term.history_size() as i32) - (history_size_before_resize as i32);

    // AIR-5316 instrumentation: capture the raw state so we can see, for the line-eating repro,
    // exactly how the cursor is moved relative to the viewport and whether a trailing blank line
    // survives. Logged at warn so it reaches fsdaemon.log without enabling debug.
    let cursor_before = term.grid.cursor.point.line.0;
    let bottommost = term.bottommost_line().0;
    log::warn!(
        "AIR-5316 conpty-adjust: history_size_change={history_size_change} cursor_before={cursor_before} bottommost={bottommost}"
    );

    if history_size_change > 0 {
        // determine number of lines we can scroll down (i.e. number of blank lines available)
        let mut scroll_lines = 0;
        while scroll_lines < history_size_change {
            let line = term.bottommost_line() - Line(scroll_lines);
            let line_text = term.line_to_string(line, Column(0)..term.last_column(), true);
            if line_text.trim().is_empty() {
                scroll_lines += 1;
            } else {
                break;
            }
        }
        if scroll_lines > 0 {
            term.scroll_down_relative(term.topmost_line(), scroll_lines as usize);
            // Clamp into the viewport: scrolling the cursor down by `scroll_lines` can push it
            // past `bottommost_line()` when the cursor is already near the bottom, and any later
            // access (damage tracking / `cursor_cell`) would then index out of bounds. (AIR-5316)
            let max_line = term.bottommost_line().0;
            let raw_new = term.grid.cursor.point.line.0 + scroll_lines;
            term.grid.cursor.point.line = Line(raw_new.clamp(0, max_line));
            term.grid.saved_cursor.point.line = Line((term.grid.saved_cursor.point.line.0 + scroll_lines).clamp(0, max_line));
            // scroll down introduces blank lines at the top of the history - remove them
            term.grid.decrease_scroll_limit(scroll_lines as usize);
            log::warn!(
                "AIR-5316 shrink-branch: scroll_lines={scroll_lines} raw_new_cursor={raw_new} clamped_cursor={} overshoot={}",
                term.grid.cursor.point.line.0,
                raw_new - max_line
            );
        }

    } else if history_size_change < 0 {
        let scroll_lines = -history_size_change;
        // scroll up introduces blank lines at the bottom of the screen (similar to scroll down)
        // however these can stay
        term.scroll_up_relative(Line(0), scroll_lines as usize);
        // Clamp into the viewport: scrolling the cursor up by `scroll_lines` can drive its line
        // negative, which underflows to a huge `usize` when indexing the grid. (AIR-5316)
        let max_line = term.bottommost_line().0;
        let raw_new = term.grid.cursor.point.line.0 - scroll_lines;
        term.grid.cursor.point.line = Line(raw_new.clamp(0, max_line));
        term.grid.saved_cursor.point.line = Line((term.grid.saved_cursor.point.line.0 - scroll_lines).clamp(0, max_line));
        log::warn!(
            "AIR-5316 grow-branch: scroll_lines={scroll_lines} raw_new_cursor={raw_new} clamped_cursor={}",
            term.grid.cursor.point.line.0
        );
    }

    // After the adjustment, report whether the bottommost line is blank and where the cursor sits,
    // so we can confirm the line-eating: cursor on a non-blank bottommost line means the next print
    // overwrites real content. (AIR-5316)
    let bottom_line = term.bottommost_line();
    let bottom_blank = term
        .line_to_string(bottom_line, Column(0)..term.last_column(), true)
        .trim()
        .is_empty();
    log::warn!(
        "AIR-5316 after-adjust: cursor_line={} bottommost={} bottom_line_blank={bottom_blank}",
        term.grid.cursor.point.line.0,
        term.bottommost_line().0
    );

    term.mark_fully_damaged();
}
