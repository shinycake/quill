use gpui::App;

mod app_menu_bar;
mod context_menu;
mod dropdown_menu;
mod menu_item;
mod popup_menu;
mod submenu_aim;

pub use app_menu_bar::AppMenuBar;
pub use context_menu::{ContextMenu, ContextMenuExt, ContextMenuState};
pub use dropdown_menu::DropdownMenu;
pub(crate) use menu_item::MenuItemElement;
pub use popup_menu::{PopupMenu, PopupMenuItem};
pub use submenu_aim::{AimGeometry, AimPointer, SUBMENU_AIM_REST, SubmenuAim};

pub(crate) fn init(cx: &mut App) {
    app_menu_bar::init(cx);
    popup_menu::init(cx);
}
