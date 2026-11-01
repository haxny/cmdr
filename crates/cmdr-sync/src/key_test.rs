use crate::*;

fn caps(case_sensitive: bool) -> SyncCaps {
    SyncCaps {
        case_sensitive,
        names_in_any_unicode_form: false,
        mtime_precision_secs: 1,
        can_write: true,
        has_trash: false,
        atomic_replace: false,
        can_read_content: false,
        renames_can_copy: false,
    }
}

#[test]
fn nfd_and_nfc_share_a_key() {
    assert_eq!(NameRule::ExactCase.key("e\u{301}"), NameRule::ExactCase.key("\u{e9}"));
    assert_eq!(NameRule::FoldCase.key("E\u{301}"), NameRule::FoldCase.key("\u{e9}"));
}

#[test]
fn fold_case_when_either_side_is_insensitive() {
    let (ci, cs) = (caps(false), caps(true));
    assert_eq!(NameRule::for_pair(&ci, &cs), NameRule::FoldCase);
    assert_eq!(NameRule::for_pair(&cs, &ci), NameRule::FoldCase);
    assert_eq!(NameRule::for_pair(&ci, &ci), NameRule::FoldCase);
    assert_eq!(NameRule::for_pair(&cs, &cs), NameRule::ExactCase);
    assert_eq!(NameRule::FoldCase.key("Report"), NameRule::FoldCase.key("report"));
    assert_ne!(NameRule::ExactCase.key("Report"), NameRule::ExactCase.key("report"));
}

#[test]
fn entry_key_orders_parents_before_children() {
    let a = EntryKey::root().child("a".into());
    let ab = a.child("b".into());
    let b = EntryKey::root().child("b".into());
    let mut keys = vec![b.clone(), ab.clone(), EntryKey::root(), a.clone()];
    keys.sort();
    assert_eq!(keys, vec![EntryKey::root(), a, ab, b]);
}

#[test]
fn entry_key_parent_and_containment() {
    let a = EntryKey::root().child("a".into());
    let ab = a.child("b".into());
    assert_eq!(ab.parent(), Some(a.clone()));
    assert_eq!(EntryKey::root().parent(), None);
    assert!(ab.is_within(&a));
    assert!(ab.is_within(&EntryKey::root()));
    assert!(a.is_within(&a));
    assert!(!a.is_within(&ab));
    assert!(!EntryKey::root().child("ab".into()).is_within(&a));
}
