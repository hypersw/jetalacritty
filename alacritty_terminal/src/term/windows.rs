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
        // Cap to the cursor's distance from the bottom: scrolling the cursor's own line out ejects
        // the trailing blank growth line into history (where the cursor can't follow), so the next
        // write overwrites real content. (AIR-5316)
        let scroll_lines = scroll_lines.min((term.bottommost_line().0 - term.grid.cursor.point.line.0).max(0));
        if scroll_lines > 0 {
            term.scroll_down_relative(term.topmost_line(), scroll_lines as usize);
            // Clamp into the viewport; an out-of-range cursor later indexes the grid out of bounds. (AIR-5316)
            let max_line = term.bottommost_line().0;
            term.grid.cursor.point.line = Line((term.grid.cursor.point.line.0 + scroll_lines).clamp(0, max_line));
            term.grid.saved_cursor.point.line = Line((term.grid.saved_cursor.point.line.0 + scroll_lines).clamp(0, max_line));
            // scroll down introduces blank lines at the top of the history - remove them
            term.grid.decrease_scroll_limit(scroll_lines as usize);
        }

    } else if history_size_change < 0 {
        let scroll_lines = -history_size_change;
        // scroll up introduces blank lines at the bottom of the screen (similar to scroll down)
        // however these can stay
        term.scroll_up_relative(Line(0), scroll_lines as usize);
        // Clamp into the viewport; a negative cursor line underflows to a huge usize on indexing. (AIR-5316)
        let max_line = term.bottommost_line().0;
        term.grid.cursor.point.line = Line((term.grid.cursor.point.line.0 - scroll_lines).clamp(0, max_line));
        term.grid.saved_cursor.point.line = Line((term.grid.saved_cursor.point.line.0 - scroll_lines).clamp(0, max_line));
    }
    term.mark_fully_damaged();
}
