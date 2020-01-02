// Copyright (C) 2018-2019 Robin Krahl <robin.krahl@ireas.org>
// SPDX-License-Identifier: MIT

use nitrokey_sys;

use crate::device::{Device, Model};
use crate::otp::GenerateOtp;

/// A Nitrokey Pro device without user or admin authentication.
///
/// Use the [`connect`][] method to obtain an instance wrapper or the [`connect_pro`] method to
/// directly obtain an instance.  If you want to execute a command that requires user or admin
/// authentication, use [`authenticate_admin`][] or [`authenticate_user`][].
///
/// # Examples
///
/// Authentication with error handling:
///
/// ```no_run
/// use nitrokey::{Authenticate, User, Pro};
/// # use nitrokey::Error;
///
/// fn perform_user_task<'a>(device: &User<'a, Pro<'a>>) {}
/// fn perform_other_task(device: &Pro) {}
///
/// # fn try_main() -> Result<(), Error> {
/// let mut manager = nitrokey::take()?;
/// let device = manager.connect_pro()?;
/// let device = match device.authenticate_user("123456") {
///     Ok(user) => {
///         perform_user_task(&user);
///         user.device()
///     },
///     Err((device, err)) => {
///         eprintln!("Could not authenticate as user: {}", err);
///         device
///     },
/// };
/// perform_other_task(&device);
/// #     Ok(())
/// # }
/// ```
///
/// [`authenticate_admin`]: trait.Authenticate.html#method.authenticate_admin
/// [`authenticate_user`]: trait.Authenticate.html#method.authenticate_user
/// [`connect`]: struct.Manager.html#method.connect
/// [`connect_pro`]: struct.Manager.html#method.connect_pro
#[derive(Debug)]
pub struct Pro<'a> {
    manager: Option<&'a mut crate::Manager>,
}

impl<'a> Pro<'a> {
    pub(crate) fn new(manager: &'a mut crate::Manager) -> Pro<'a> {
        Pro {
            manager: Some(manager),
        }
    }
}

impl<'a> Drop for Pro<'a> {
    fn drop(&mut self) {
        unsafe {
            nitrokey_sys::NK_logout();
        }
    }
}

impl<'a> Device<'a> for Pro<'a> {
    fn into_manager(mut self) -> &'a mut crate::Manager {
        self.manager.take().unwrap()
    }

    fn get_model(&self) -> Model {
        Model::Pro
    }
}

impl<'a> GenerateOtp for Pro<'a> {}
