export type Modifier = "none" | "shift" | "ctrl" | "alt" | "altgr" | "shift_ctrl";

export function parseModifier(flags: {
  shift: boolean;
  ctrl: boolean;
  alt: boolean;
  altgr: boolean;
}): Modifier {
  if (flags.altgr) return "altgr";
  if (flags.shift && flags.ctrl) return "shift_ctrl";
  if (flags.shift) return "shift";
  if (flags.ctrl) return "ctrl";
  if (flags.alt) return "alt";
  return "none";
}
