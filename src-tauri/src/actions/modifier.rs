use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    #[default]
    None,
    Shift,
    Ctrl,
    Alt,
    Altgr,
    ShiftCtrl,
}

pub fn from_flags(shift: bool, ctrl: bool, alt: bool, altgr: bool) -> Modifier {
    if altgr {
        return Modifier::Altgr;
    }
    if shift && ctrl {
        Modifier::ShiftCtrl
    } else if shift {
        Modifier::Shift
    } else if ctrl {
        Modifier::Ctrl
    } else if alt {
        Modifier::Alt
    } else {
        Modifier::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_modifiers() {
        assert_eq!(from_flags(false, false, false, false), Modifier::None);
    }

    #[test]
    fn shift_only() {
        assert_eq!(from_flags(true, false, false, false), Modifier::Shift);
    }

    #[test]
    fn ctrl_only() {
        assert_eq!(from_flags(false, true, false, false), Modifier::Ctrl);
    }

    #[test]
    fn alt_only() {
        assert_eq!(from_flags(false, false, true, false), Modifier::Alt);
    }

    #[test]
    fn altgr_takes_precedence() {
        assert_eq!(from_flags(true, true, true, true), Modifier::Altgr);
    }

    #[test]
    fn shift_ctrl_combo() {
        assert_eq!(from_flags(true, true, false, false), Modifier::ShiftCtrl);
    }

    #[test]
    fn deserialize_none() {
        let m: Modifier = serde_json::from_str(r#""none""#).unwrap();
        assert_eq!(m, Modifier::None);
    }

    #[test]
    fn deserialize_shift() {
        let m: Modifier = serde_json::from_str(r#""shift""#).unwrap();
        assert_eq!(m, Modifier::Shift);
    }

    #[test]
    fn deserialize_ctrl() {
        let m: Modifier = serde_json::from_str(r#""ctrl""#).unwrap();
        assert_eq!(m, Modifier::Ctrl);
    }

    #[test]
    fn shift_alt_yields_shift() {
        assert_eq!(from_flags(true, false, true, false), Modifier::Shift);
    }

    #[test]
    fn ctrl_alt_yields_ctrl() {
        assert_eq!(from_flags(false, true, true, false), Modifier::Ctrl);
    }

    #[test]
    fn shift_ctrl_alt_yields_shift_ctrl() {
        assert_eq!(from_flags(true, true, true, false), Modifier::ShiftCtrl);
    }
}
