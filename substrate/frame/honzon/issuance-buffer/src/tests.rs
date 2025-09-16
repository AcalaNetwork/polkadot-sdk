// Basic tests for pallet-issuance-buffer

use crate::mock::*;
use frame_support::assert_ok;

#[test]
fn it_works_for_default_value() {
    new_test_ext().execute_with(|| {
        // Just a simple test to ensure everything is wired up
        assert_ok!(IssuanceBuffer::fund(RuntimeOrigin::root(), 100));
    });
}
