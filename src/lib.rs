// Copyright(c) 2026 3NSoft Inc.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Lesser General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

#![deny(clippy::all)]

mod js_callbacks;
mod fs_impl;

use std::{path::Path, sync::mpsc::{Sender, channel}, thread};

use napi::bindgen_prelude::*;
use napi_derive::napi;
use fuser::{Config, MountOption, SessionACL, spawn_mount2};

use crate::{fs_impl::CallbacksProxy, js_callbacks::*};

#[napi(js_name = "FSMounter")]
pub struct JsFSMounter {
  tx_unmount_signal: Sender<()>
}

#[napi]
impl JsFSMounter {

  #[napi(factory)]
  pub fn make_and_mount(
    mount_root: String, fs_name: String,
    init: InitOpCB,
    destroy: DestroyOpCB,
    lookup: LookupOpCB,
    forget: ForgetOpCB,
    getattr: GetAttrOpCB,
    setattr: SetAttrOpCB,
    open: OpenOpCB,
    read: ReadOpCB,
    release: ReleaseOpCB,
    opendir: OpenDirOpCB,
    readdir: ReadDirOpCB,
    releasedir: ReleaseDirOpCB,
    getxattr: GetXAttrOpCB,
    listxattr: ListXAttrOpCB,
    access: AccessOpCB,
  ) -> Result<Self> {

    let fs_impl = CallbacksProxy::make(CallbacksToJS {
      init, destroy, lookup, forget, getattr, setattr, open, read, release, opendir, readdir, releasedir,
      getxattr, listxattr, access,
    });

    let (tx_unmount_signal, rx_unmount_signal) = channel::<()>();

    thread::spawn(move || {
      let mut cfg = Config::default();
      cfg.mount_options.extend([MountOption::RO, MountOption::FSName(fs_name)]);
      cfg.acl = SessionACL::Owner;
      let mounting = spawn_mount2(fs_impl, Path::new(&mount_root), &cfg);
      match mounting {
        Ok(mount_session) => {
          rx_unmount_signal.recv().unwrap_or(());
          let _ = mount_session.join();
        },
        _ => ()
      }
    });

    Ok(JsFSMounter { tx_unmount_signal })
  }

  #[napi]
  pub fn unmount(&mut self) -> Result<()> {
    let _ = self.tx_unmount_signal.send(());
    Ok(())
  }

}
