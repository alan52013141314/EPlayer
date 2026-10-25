use std::collections::BTreeSet;

#[derive(Default)]
pub struct RowSelection {
    pub rows: BTreeSet<usize>,
    anchor: Option<usize>,
}
impl RowSelection {
    pub fn click(&mut self, row: usize, ctrl: bool, shift: bool) {
        if shift {
            let anchor = self.anchor.unwrap_or(row);
            if !ctrl {
                self.rows.clear();
            }
            self.rows.extend(anchor.min(row)..=anchor.max(row));
            self.anchor = Some(anchor);
        } else {
            if !ctrl {
                self.rows.clear();
            }
            if !self.rows.insert(row) && ctrl {
                self.rows.remove(&row);
            }
            self.anchor = Some(row);
        }
    }
    pub fn right_click(&mut self, row: usize) {
        if !self.rows.contains(&row) {
            self.click(row, false, false);
        }
    }
    pub fn select_all(&mut self, len: usize) {
        self.rows = (0..len).collect();
        self.anchor = if len > 0 { Some(0) } else { None };
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_ctrl_shift_and_context_selection() {
        let mut s = RowSelection::default();
        s.click(2, false, false);
        s.click(5, false, true);
        assert_eq!(s.rows.iter().copied().collect::<Vec<_>>(), vec![2, 3, 4, 5]);
        s.click(3, true, false);
        assert!(!s.rows.contains(&3));
        s.click(8, true, false);
        s.right_click(4);
        assert_eq!(s.rows.len(), 4);
        s.right_click(1);
        assert_eq!(s.rows.iter().copied().collect::<Vec<_>>(), vec![1]);
        s.click(3, true, true);
        assert_eq!(s.rows.iter().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
        s.select_all(120);
        assert_eq!(s.rows.len(), 120);
        s.click(105, true, false);
        assert_eq!(s.rows.len(), 119);
        s.clear();
        s.click(6, false, true);
        assert_eq!(s.rows.iter().copied().collect::<Vec<_>>(), vec![6]);
    }
}
