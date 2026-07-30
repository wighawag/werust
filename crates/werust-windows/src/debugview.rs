//! The debug view: a SEPARATE window with a tabbed CONSOLE + NETWORK list over
//! the shared capture store.
//!
//! A separate window rather than an in-window panel — the same choice the GTK and
//! AppKit edges made and recorded: a panel would crowd the page on every open,
//! and a window closes with its own close button. It is READ-ONLY (a list view of
//! rows, never an input); a typeable REPL is the DevTools window's job, and real
//! Chrome DevTools are one ⋮-menu entry away here (`OpenDevToolsWindow`).
//!
//! Every row's TEXT and every row's COLOUR CLASS come from the shared carrier
//! (which reads the core's `console_row_text` / `network_*` rules), so these tabs
//! read exactly like the GTK and macOS ones. This file only inserts list items
//! and answers `NM_CUSTOMDRAW` with the colour the carrier chose.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use windows::core::PWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

use crate::paint::{ConsoleRowPaint, NetworkRowPaint, TabUpdate};
use crate::win32::{colorref, wide};

/// `LVM_*`, `LVIF_*` and `LVCF_*`: the list-view messages and masks this view
/// uses. Spelled here rather than imported so the control vocabulary is visible
/// in one place (and so a `windows`-crate feature change cannot silently drop
/// one).
const LVM_FIRST: u32 = 0x1000;
const LVM_DELETEITEM: u32 = LVM_FIRST + 8;
const LVM_DELETEALLITEMS: u32 = LVM_FIRST + 9;
const LVM_GETITEMCOUNT: u32 = LVM_FIRST + 4;
const LVM_INSERTCOLUMNW: u32 = LVM_FIRST + 97;
const LVM_INSERTITEMW: u32 = LVM_FIRST + 77;
const LVM_SETITEMW: u32 = LVM_FIRST + 76;
const LVIF_TEXT: u32 = 0x0001;
const LVCF_WIDTH: u32 = 0x0002;
const LVCF_TEXT: u32 = 0x0004;

/// The `LVITEMW` layout, spelled locally because only these fields are used and
/// the struct must match the Win32 ABI exactly.
#[repr(C)]
#[derive(Default)]
struct LvItemW {
    mask: u32,
    i_item: i32,
    i_sub_item: i32,
    state: u32,
    state_mask: u32,
    psz_text: PWSTR,
    cch_text_max: i32,
    i_image: i32,
    l_param: isize,
    i_indent: i32,
    i_group_id: i32,
    c_columns: u32,
    pu_columns: *mut u32,
    pi_col_fmt: *mut i32,
    i_group: i32,
}

/// The `LVCOLUMNW` layout, likewise.
#[repr(C)]
#[derive(Default)]
struct LvColumnW {
    mask: u32,
    fmt: i32,
    cx: i32,
    psz_text: PWSTR,
    cch_text_max: i32,
    i_sub_item: i32,
    i_image: i32,
    i_order: i32,
    cx_min: i32,
    cx_default: i32,
    cx_ideal_header: i32,
}

/// The CONSOLE tab's index.
pub const TAB_CONSOLE: i32 = 0;
/// The NETWORK tab's index.
pub const TAB_NETWORK: i32 = 1;
/// The NETWORK list's TRUST column — the one that wears the chrome's own posture
/// colour (`docs/adr/0006`: the debug view never mints a second trust label).
pub const NETWORK_TRUST_COLUMN: i32 = 4;

/// One tab: a list view, its per-row colours, and the two anchors its incremental
/// refresh needs.
pub struct DebugTab {
    /// The list view itself.
    pub list: HWND,
    /// One colour per rendered row, kept in step with the list so
    /// `NM_CUSTOMDRAW` can answer for row N without re-deriving anything.
    pub colors: RefCell<VecDeque<COLORREF>>,
    /// How many rows the view holds (the carrier's incremental-refresh anchor).
    pub rendered: Cell<usize>,
    /// The sequence of the last store entry rendered (the other anchor —
    /// sequence-anchored, so ring-buffer eviction at the cap cannot freeze the
    /// view).
    pub last_sequence: Cell<Option<u64>>,
}

impl DebugTab {
    /// A tab over an existing list view.
    #[must_use]
    pub fn new(list: HWND) -> Self {
        Self {
            list,
            colors: RefCell::new(VecDeque::new()),
            rendered: Cell::new(0),
            last_sequence: Cell::new(None),
        }
    }

    /// How many rows the list ACTUALLY holds (asked of the control, not of our
    /// bookkeeping — which is what makes the CI smoke's assertion meaningful).
    #[must_use]
    pub fn row_count(&self) -> usize {
        let count = unsafe {
            SendMessageW(
                self.list,
                LVM_GETITEMCOUNT,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            )
        };
        usize::try_from(count.0).unwrap_or(0)
    }

    /// The colour to draw row `index` in, if the tab has one.
    #[must_use]
    pub fn color_of(&self, index: usize) -> Option<COLORREF> {
        self.colors.borrow().get(index).copied()
    }

    /// Add a column to the list.
    pub fn add_column(&self, index: i32, title: &str, width: i32) {
        let mut text = wide(title);
        let mut column = LvColumnW {
            mask: LVCF_TEXT | LVCF_WIDTH,
            cx: width,
            psz_text: PWSTR(text.as_mut_ptr()),
            i_sub_item: index,
            ..Default::default()
        };
        unsafe {
            SendMessageW(
                self.list,
                LVM_INSERTCOLUMNW,
                Some(WPARAM(index as usize)),
                Some(LPARAM(std::ptr::from_mut(&mut column) as isize)),
            );
        }
    }

    /// Append one row of columns, in the colour the carrier chose.
    fn push_row(&self, columns: &[&str], color: COLORREF) {
        let index = i32::try_from(self.row_count()).unwrap_or(i32::MAX);
        for (sub, text) in columns.iter().enumerate() {
            let mut buffer = wide(text);
            let mut item = LvItemW {
                mask: LVIF_TEXT,
                i_item: index,
                i_sub_item: i32::try_from(sub).unwrap_or(0),
                psz_text: PWSTR(buffer.as_mut_ptr()),
                ..Default::default()
            };
            let message = if sub == 0 {
                LVM_INSERTITEMW
            } else {
                LVM_SETITEMW
            };
            unsafe {
                SendMessageW(
                    self.list,
                    message,
                    Some(WPARAM(0)),
                    Some(LPARAM(std::ptr::from_mut(&mut item) as isize)),
                );
            }
        }
        self.colors.borrow_mut().push_back(color);
    }

    /// Remove every row.
    fn clear_rows(&self) {
        unsafe {
            SendMessageW(
                self.list,
                LVM_DELETEALLITEMS,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            );
        }
        self.colors.borrow_mut().clear();
    }

    /// Remove `count` rows from the TOP (the ones the ring buffer evicted).
    fn drop_front(&self, count: usize) {
        for _ in 0..count {
            unsafe {
                SendMessageW(self.list, LVM_DELETEITEM, Some(WPARAM(0)), Some(LPARAM(0)));
            }
            self.colors.borrow_mut().pop_front();
        }
    }

    /// Apply one CONSOLE update: the carrier says what to do, this puts it in the
    /// list.
    pub fn apply_console(&self, update: TabUpdate<ConsoleRowPaint>) {
        match update {
            TabUpdate::Noop => {}
            TabUpdate::Rebuild(rows) => {
                self.clear_rows();
                for row in &rows {
                    self.push_row(&[row.text.as_str()], colorref(row.color));
                }
            }
            TabUpdate::Append { drop, rows } => {
                self.drop_front(drop);
                for row in &rows {
                    self.push_row(&[row.text.as_str()], colorref(row.color));
                }
            }
        }
    }

    /// Apply one NETWORK update. The stored colour is the TRUST column's, which
    /// is the only coloured cell in the row.
    pub fn apply_network(&self, update: TabUpdate<NetworkRowPaint>) {
        match update {
            TabUpdate::Noop => {}
            TabUpdate::Rebuild(rows) => {
                self.clear_rows();
                for row in &rows {
                    self.push_network_row(row);
                }
            }
            TabUpdate::Append { drop, rows } => {
                self.drop_front(drop);
                for row in &rows {
                    self.push_network_row(row);
                }
            }
        }
    }

    fn push_network_row(&self, row: &NetworkRowPaint) {
        self.push_row(
            &[
                row.method.as_str(),
                row.status.as_str(),
                row.mime.as_str(),
                row.size.as_str(),
                row.trust.as_str(),
                row.url.as_str(),
            ],
            colorref(row.trust_color),
        );
    }
}

/// The debug window: a tab control over the two lists, plus a CLEAR button that
/// empties the SHARED store.
pub struct DebugWindow {
    /// The window itself.
    pub window: HWND,
    /// The tab control.
    pub tabs: HWND,
    /// The CLEAR button.
    pub clear: HWND,
    pub console: DebugTab,
    pub network: DebugTab,
    /// Which tab is showing (only its list is visible: two lists in one
    /// rectangle, swapped on `TCN_SELCHANGE`).
    pub selected: Cell<i32>,
}

impl DebugWindow {
    /// The tab that owns `list`, if any (the `NM_CUSTOMDRAW` notification
    /// arrives at this window, naming the list it came from).
    #[must_use]
    pub fn tab_of(&self, list: HWND) -> Option<&DebugTab> {
        if list == self.console.list {
            Some(&self.console)
        } else if list == self.network.list {
            Some(&self.network)
        } else {
            None
        }
    }
}

/// `TCM_*` and `TCIF_*`: the tab control's messages and masks.
pub const TCM_FIRST: u32 = 0x1300;
pub const TCM_GETCURSEL: u32 = TCM_FIRST + 11;
pub const TCM_INSERTITEMW: u32 = TCM_FIRST + 62;
pub const TCIF_TEXT: u32 = 0x0001;

/// The `TCITEMW` layout.
#[repr(C)]
#[derive(Default)]
pub struct TcItemW {
    pub mask: u32,
    pub dw_state: u32,
    pub dw_state_mask: u32,
    pub psz_text: PWSTR,
    pub cch_text_max: i32,
    pub i_image: i32,
    pub l_param: isize,
}

/// Add one named tab.
pub fn add_tab(tabs: HWND, index: i32, title: &str) {
    let mut text = wide(title);
    let mut item = TcItemW {
        mask: TCIF_TEXT,
        psz_text: PWSTR(text.as_mut_ptr()),
        ..Default::default()
    };
    unsafe {
        SendMessageW(
            tabs,
            TCM_INSERTITEMW,
            Some(WPARAM(index as usize)),
            Some(LPARAM(std::ptr::from_mut(&mut item) as isize)),
        );
    }
}

/// Which tab is selected.
#[must_use]
pub fn current_tab(tabs: HWND) -> i32 {
    let selected = unsafe { SendMessageW(tabs, TCM_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))) };
    i32::try_from(selected.0).unwrap_or(TAB_CONSOLE)
}
