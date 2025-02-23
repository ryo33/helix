use crate::graphemes::next_grapheme_boundary;
use crate::{search, Selection};
use ropey::RopeSlice;

pub const PAIRS: &[(char, char)] = &[
    ('(', ')'),
    ('[', ']'),
    ('{', '}'),
    ('<', '>'),
    ('«', '»'),
    ('「', '」'),
    ('（', '）'),
];

/// Given any char in [PAIRS], return the open and closing chars. If not found in
/// [PAIRS] return (ch, ch).
///
/// ```
/// use helix_core::surround::get_pair;
///
/// assert_eq!(get_pair('['), ('[', ']'));
/// assert_eq!(get_pair('}'), ('{', '}'));
/// assert_eq!(get_pair('"'), ('"', '"'));
/// ```
pub fn get_pair(ch: char) -> (char, char) {
    PAIRS
        .iter()
        .find(|(open, close)| *open == ch || *close == ch)
        .copied()
        .unwrap_or((ch, ch))
}

/// Find the position of surround pairs of `ch` which can be either a closing
/// or opening pair. `n` will skip n - 1 pairs (eg. n=2 will discard (only)
/// the first pair found and keep looking)
pub fn find_nth_pairs_pos(
    text: RopeSlice,
    ch: char,
    pos: usize,
    n: usize,
) -> Option<(usize, usize)> {
    let (open, close) = get_pair(ch);

    if text.len_chars() < 2 || pos >= text.len_chars() {
        return None;
    }

    if open == close {
        if Some(open) == text.get_char(pos) {
            // Special case: cursor is directly on a matching char.
            match pos {
                0 => Some((pos, search::find_nth_next(text, close, pos + 1, n)? + 1)),
                _ if (pos + 1) == text.len_chars() => {
                    Some((search::find_nth_prev(text, open, pos, n)?, text.len_chars()))
                }
                // We return no match because there's no way to know which
                // side of the char we should be searching on.
                _ => None,
            }
        } else {
            Some((
                search::find_nth_prev(text, open, pos, n)?,
                search::find_nth_next(text, close, pos, n)? + 1,
            ))
        }
    } else {
        Some((
            find_nth_open_pair(text, open, close, pos, n)?,
            next_grapheme_boundary(text, find_nth_close_pair(text, open, close, pos, n)?),
        ))
    }
}

fn find_nth_open_pair(
    text: RopeSlice,
    open: char,
    close: char,
    mut pos: usize,
    n: usize,
) -> Option<usize> {
    let mut chars = text.chars_at(pos + 1);

    // Adjusts pos for the first iteration, and handles the case of the
    // cursor being *on* the close character which will get falsely stepped over
    // if not skipped here
    if chars.prev()? == open {
        return Some(pos);
    }

    for _ in 0..n {
        let mut step_over: usize = 0;

        loop {
            let c = chars.prev()?;
            pos = pos.saturating_sub(1);

            // ignore other surround pairs that are enclosed *within* our search scope
            if c == close {
                step_over += 1;
            } else if c == open {
                if step_over == 0 {
                    break;
                }

                step_over = step_over.saturating_sub(1);
            }
        }
    }

    Some(pos)
}

fn find_nth_close_pair(
    text: RopeSlice,
    open: char,
    close: char,
    mut pos: usize,
    n: usize,
) -> Option<usize> {
    if pos >= text.len_chars() {
        return None;
    }

    let mut chars = text.chars_at(pos);

    if chars.next()? == close {
        return Some(pos);
    }

    for _ in 0..n {
        let mut step_over: usize = 0;

        loop {
            let c = chars.next()?;
            pos += 1;

            if c == open {
                step_over += 1;
            } else if c == close {
                if step_over == 0 {
                    break;
                }

                step_over = step_over.saturating_sub(1);
            }
        }
    }

    Some(pos)
}

/// Find position of surround characters around every cursor. Returns None
/// if any positions overlap. Note that the positions are in a flat Vec.
/// Use get_surround_pos().chunks(2) to get matching pairs of surround positions.
/// `ch` can be either closing or opening pair.
pub fn get_surround_pos(
    text: RopeSlice,
    selection: &Selection,
    ch: char,
    skip: usize,
) -> Option<Vec<usize>> {
    let mut change_pos = Vec::new();

    for range in selection {
        let (open_pos, close_pos) = find_nth_pairs_pos(text, ch, range.head, skip)?;
        if change_pos.contains(&open_pos) || change_pos.contains(&close_pos) {
            return None;
        }
        change_pos.extend_from_slice(&[open_pos, close_pos]);
    }
    Some(change_pos)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Range;

    use ropey::Rope;
    use smallvec::SmallVec;

    #[test]
    fn test_find_nth_pairs_pos() {
        let doc = Rope::from("some (text) here");
        let slice = doc.slice(..);

        // cursor on [t]ext
        assert_eq!(find_nth_pairs_pos(slice, '(', 6, 1), Some((5, 11)));
        assert_eq!(find_nth_pairs_pos(slice, ')', 6, 1), Some((5, 11)));
        // cursor on so[m]e
        assert_eq!(find_nth_pairs_pos(slice, '(', 2, 1), None);
        // cursor on bracket itself
        assert_eq!(find_nth_pairs_pos(slice, '(', 5, 1), Some((5, 11)));
        assert_eq!(find_nth_pairs_pos(slice, '(', 10, 1), Some((5, 11)));
    }

    #[test]
    fn test_find_nth_pairs_pos_skip() {
        let doc = Rope::from("(so (many (good) text) here)");
        let slice = doc.slice(..);

        // cursor on go[o]d
        assert_eq!(find_nth_pairs_pos(slice, '(', 13, 1), Some((10, 16)));
        assert_eq!(find_nth_pairs_pos(slice, '(', 13, 2), Some((4, 22)));
        assert_eq!(find_nth_pairs_pos(slice, '(', 13, 3), Some((0, 28)));
    }

    #[test]
    fn test_find_nth_pairs_pos_same() {
        let doc = Rope::from("'so 'many 'good' text' here'");
        let slice = doc.slice(..);

        // cursor on go[o]d
        assert_eq!(find_nth_pairs_pos(slice, '\'', 13, 1), Some((10, 16)));
        assert_eq!(find_nth_pairs_pos(slice, '\'', 13, 2), Some((4, 22)));
        assert_eq!(find_nth_pairs_pos(slice, '\'', 13, 3), Some((0, 28)));
        // cursor on the quotes
        assert_eq!(find_nth_pairs_pos(slice, '\'', 10, 1), None);
        // this is the best we can do since opening and closing pairs are same
        assert_eq!(find_nth_pairs_pos(slice, '\'', 0, 1), Some((0, 5)));
        assert_eq!(find_nth_pairs_pos(slice, '\'', 27, 1), Some((21, 28)));
    }

    #[test]
    fn test_find_nth_pairs_pos_step() {
        let doc = Rope::from("((so)((many) good (text))(here))");
        let slice = doc.slice(..);

        // cursor on go[o]d
        assert_eq!(find_nth_pairs_pos(slice, '(', 15, 1), Some((5, 25)));
        assert_eq!(find_nth_pairs_pos(slice, '(', 15, 2), Some((0, 32)));
    }

    #[test]
    fn test_find_nth_pairs_pos_mixed() {
        let doc = Rope::from("(so [many {good} text] here)");
        let slice = doc.slice(..);

        // cursor on go[o]d
        assert_eq!(find_nth_pairs_pos(slice, '{', 13, 1), Some((10, 16)));
        assert_eq!(find_nth_pairs_pos(slice, '[', 13, 1), Some((4, 22)));
        assert_eq!(find_nth_pairs_pos(slice, '(', 13, 1), Some((0, 28)));
    }

    #[test]
    fn test_get_surround_pos() {
        let doc = Rope::from("(some) (chars)\n(newline)");
        let slice = doc.slice(..);
        let selection = Selection::new(
            SmallVec::from_slice(&[Range::point(2), Range::point(9), Range::point(20)]),
            0,
        );

        // cursor on s[o]me, c[h]ars, newl[i]ne
        assert_eq!(
            get_surround_pos(slice, &selection, '(', 1)
                .unwrap()
                .as_slice(),
            &[0, 6, 7, 14, 15, 24]
        );
    }

    #[test]
    fn test_get_surround_pos_bail() {
        let doc = Rope::from("[some]\n(chars)xx\n(newline)");
        let slice = doc.slice(..);

        let selection =
            Selection::new(SmallVec::from_slice(&[Range::point(2), Range::point(9)]), 0);

        // cursor on s[o]me, c[h]ars
        assert_eq!(
            get_surround_pos(slice, &selection, '(', 1),
            None // different surround chars
        );

        let selection = Selection::new(
            SmallVec::from_slice(&[Range::point(14), Range::point(24)]),
            0,
        );
        // cursor on [x]x, newli[n]e
        assert_eq!(
            get_surround_pos(slice, &selection, '(', 1),
            None // overlapping surround chars
        );
    }
}
