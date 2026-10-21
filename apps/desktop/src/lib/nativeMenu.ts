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

/// What makes two item lists the same native menu: their shape, labels and
/// enabled state. Click handlers are deliberately not part of it -- they
/// change with every render and are looked up at click time (see `Entry`).
function shapeKey(items: ContextMenuItem[]): string {
  return JSON.stringify(
    items.map(function walk(item): unknown {
      return [item.label, !!item.disabled, item.submenu?.map(walk)];
    }),
  );
}

/// The click handlers of `items` in the order `toNative` visits them.
function handlersOf(items: ContextMenuItem[]): Array<(() => void) | undefined> {
  return items.flatMap((item) => (item.submenu ? handlersOf(item.submenu) : [item.onClick]));
}

/// A native menu plus the handlers its items call. The menu is built once
/// per shape; each popup swaps in the current handlers, so a reused menu
/// never runs a click handler from a render that is gone.
interface Entry {
  menu: Menu;
  handlers: Array<(() => void) | undefined>;
}

function toNative(items: ContextMenuItem[], entry: { handlers: Entry["handlers"] }) {
  let next = 0;
  const build = (item: ContextMenuItem): MenuItemOptions | SubmenuOptions => {
    if (item.submenu) {
      return { text: item.label, enabled: !item.disabled, items: item.submenu.map(build) };
    }
    const index = next++;
    return {
      text: item.label,
      enabled: !item.disabled,
      action: () => entry.handlers[index]?.(),
    };
  };
  return items.map(build);
}

// Tauri registers a channel for every menu and item it creates and never
// removes it (`close()` frees the menu itself, not those channels), so a
// menu built per popup would grow the host's table for as long as the app
// runs. Menus are therefore built once per distinct shape and reused. The
// cache is least-recently-used and capped, so even a long session that sees
// many shapes (a menu lists project and section names) stays bounded: an
// evicted menu is closed and rebuilt if its shape comes back.
const MAX_CACHED_MENUS = 24;
const cache = new Map<string, Promise<Entry>>();

function menuFor(items: ContextMenuItem[]): Promise<Entry> {
  const key = shapeKey(items);
  const hit = cache.get(key);
  if (hit) {
    cache.delete(key);
    cache.set(key, hit);
    return hit;
  }
  const entry = { handlers: [] as Entry["handlers"] };
  const created = Menu.new({ items: toNative(items, entry) }).then(
    (menu) => Object.assign(entry, { menu }) as Entry,
  );
  // A failed build must not stay cached as a failure.
  created.catch(() => cache.get(key) === created && cache.delete(key));
  cache.set(key, created);
  while (cache.size > MAX_CACHED_MENUS) {
    const [oldest] = cache.keys();
    const evicted = cache.get(oldest);
    cache.delete(oldest);
    // Its item actions are delivered after `popup` resolves; by the time
    // 24 other menus have opened, that is long done.
    void evicted?.then((e) => e.menu.close()).catch(() => {});
  }
  return created;
}

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
  const entry = await menuFor(items);
  entry.handlers = handlersOf(items);
  const byMouse = Date.now() - lastPointerAt < POINTER_FRESH_MS;
  const { menu } = entry;
  await (byMouse ? menu.popup() : menu.popup(new LogicalPosition(at.x, at.y)));
}
