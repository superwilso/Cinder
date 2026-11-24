use super::*;

/// The bottom-edge swipe opens the Shelf from an ordinary screen.
#[test]
fn a_bottom_edge_swipe_opens_the_shelf() {
    let mut a = App::unlocked();
    assert!(!a.shelf_is_open());
    assert!(a.shelf_swipe_open(), "the gesture should have been taken");
    assert!(a.shelf_is_open());
}

/// It declines rather than stacking when the Shelf is already up — a second swipe on an open
/// Shelf is far more likely to be someone trying to dismiss it than to open it again.
#[test]
fn a_second_swipe_is_declined_while_the_shelf_is_open() {
    let mut a = App::unlocked();
    assert!(a.shelf_swipe_open());
    assert!(
        !a.shelf_swipe_open(),
        "must decline, so the shell can let the contact scroll"
    );
    assert!(a.shelf_is_open(), "and must not have closed it either");
}

/// Locked means locked: nothing should be reachable behind the lock screen by gesture.
#[test]
fn the_lock_screen_declines_the_gesture() {
    let mut a = App::unlocked();
    a.push_for_test(Screen::Lock);
    assert!(
        !a.shelf_swipe_open(),
        "the Shelf must not open over the lock screen"
    );
    assert!(!a.shelf_is_open());
}
