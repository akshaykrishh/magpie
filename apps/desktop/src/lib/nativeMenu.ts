import { LogicalPosition } from "@tauri-apps/api/dpi";
import { Menu, type MenuItemOptions, type SubmenuOptions } from "@tauri-apps/api/menu";

export interface ContextMenuItem {
  label: string;
  onClick?: () => void;
  disabled?: boolean;
  /** Opens a nested native submenu; a parent with one has no action of its own. */
  submenu?: ContextMenuItem[];
  /** Kept on the item for callers' own bookkeeping; a native menu has no
      red text, so it is not drawn. */
  destructive?: boolean;
}

function toNative(item: ContextMenuItem): MenuItemOptions | SubmenuOptions {
  if (item.submenu) {
    return {
      text: item.label,
      enabled: !item.disabled,
      items: item.submenu.map(toNative),
    };
  }
  return {
    text: item.label,
    enabled: !item.disabled,
    action: () => item.onClick?.(),
  };
}

// The previous menu stays alive until the next one opens: its item actions
// are delivered after `popup` resolves, so closing it any earlier could drop
// the click that just chose something.
let last: Menu | null = null;

// When the pointer last pressed or right-clicked in this window. A menu
// opened by the mouse belongs at the cursor -- exactly where the click was --
// and the cursor is the one position the OS reports without any conversion
// between the webview's coordinates and the window's.
let lastPointerAt = 0;
if (typeof document !== "undefined") {
  for (const type of ["pointerdown", "contextmenu"]) {
    document.addEventListener(type, () => (lastPointerAt = Date.now()), true);
  }
}
const POINTER_FRESH_MS = 500;

/// Shows `items` as a native OS menu: at the cursor when a mouse click just
/// opened it (the "..." button, a right-click), otherwise at `(x, y)` (CSS
/// pixels from the webview's top-left, for the keyboard's menu key). Native,
/// not HTML, so it is never clipped or covered by the
/// window it was opened from -- the Dock is a 300px always-on-top window --
/// and it gets the system's own keyboard navigation and accessibility.
export async function popupMenu(items: ContextMenuItem[], at: { x: number; y: number }) {
  if (!("__TAURI_INTERNALS__" in window)) return;
  const menu = await Menu.new({ items: items.map(toNative) });
  const previous = last;
  last = menu;
  void previous?.close();
  const byMouse = Date.now() - lastPointerAt < POINTER_FRESH_MS;
  await (byMouse ? menu.popup() : menu.popup(new LogicalPosition(at.x, at.y)));
}
