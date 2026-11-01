use super::*;

fn s() -> &'static mut State {
    // SAFETY: the test owns the state and runs on the thread that created it.
    unsafe { state() }
}

#[test]
#[ignore = "Creates the tray window"]
fn text_is_extracted_from_screenshots_only() {
    let _desk = crate::test_support::desk::Desk::start();
    for (extract, video, expected) in [
        (false, false, false),
        (true, false, true),
        (true, true, false),
        (false, true, false),
    ] {
        s().extract = extract;
        s().video = video;
        // SAFETY: plain reads of the test's own state.
        assert_eq!(unsafe { wanted() }, expected, "{extract} {video}");
    }
}
