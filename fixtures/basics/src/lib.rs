/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[uniffi::export]
fn get_string() -> String {
    "String created by Rust".to_owned()
}

#[uniffi::export]
fn get_int() -> i32 {
    1289
}

#[uniffi::export]
fn pass_string(_s: String) {}
