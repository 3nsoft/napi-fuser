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

use std::{ffi::OsStr, path::Path, sync::mpsc::channel, time::{Duration, SystemTime}};
use libc::{ENOSYS, EPERM, c_int};

use fuser::{FUSE_ROOT_ID, Filesystem, KernelConfig, PollHandle, ReplyAttr, ReplyBmap, ReplyCreate, ReplyData, ReplyDirectory, ReplyDirectoryPlus, ReplyEmpty, ReplyEntry, ReplyIoctl, ReplyLock, ReplyLseek, ReplyOpen, ReplyPoll, ReplyStatfs, ReplyWrite, ReplyXattr, Request, TimeOrNow, fuse_forget_one};
use napi::threadsafe_function::ThreadsafeFunctionCallMode;

use crate::js_callbacks::*;

/// This keeps js functions for providing FUSE implementation that [`fuser`] mounts into OS.
/// 
/// This has [`Filesystem`] implemented.
/// Implemented functions are invoked in [`fuser`]'s thread.
/// Yet, any callbacks to process returned from js side NAPI values are invoked in NAPI-RS env(ironment).
/// Such setup adds no additional threads/runtimes.
pub struct CallbacksProxy {
  cbs: CallbacksToJS,
}

impl CallbacksProxy {

  pub fn make(cbs: CallbacksToJS) -> CallbacksProxy {
    CallbacksProxy { cbs }
  }

}

/// This calls js functions, with following patterns, corresponding to arms of this macro:
/// - **arm #0** - calling without arguments a sync function.
///   This needs only js function. Macro expands into statement.
/// - **arm #1** - calling with arguments a sync function.
///   This needs js function and tuple of arguments. Macro expands into statement.
/// - **arm #2** - calling with arguments an async function.
///   This needs js function, tuple of arguments, type of return data and a channel to pass data from NAPI side.
///   Macro expands into expression of returned data.
/// - **arm #3** - calling with arguments an async function.
///   
/// 
macro_rules! call_js {
  ($js_fn:expr) => {
    $js_fn.call(Ok(()), ThreadsafeFunctionCallMode::Blocking);
  };
  ($js_fn:expr, $args:expr) => {
    $js_fn.call(Ok($args.into()), ThreadsafeFunctionCallMode::Blocking);
  };
  ($js_fn:expr, $args:expr, $out_type:ty, $reply:ident, @initial-thread => $with_reply:expr) => {
    {
      let (tx_done_signal, rx_done_signal) = channel::<Option<$out_type>>();
      $js_fn.call_with_return_value(
        Ok($args.into()),
        ThreadsafeFunctionCallMode::Blocking,
        move |js_reply, env| {
          match js_reply {
            Ok(js_reply) => {
              let _ = env.spawn_future(async move {
                let _ = match js_reply.await {
                  Ok(js_reply) => tx_done_signal.send(Some(js_reply)),
                  Err(_) => tx_done_signal.send(None),
                };
                Ok(())
              });
            },
            Err(_) => {
              let _ = tx_done_signal.send(None);
            }
          };
          Ok(())
        }
      );
      match rx_done_signal.recv_timeout(Duration::from_secs(30)) {
        Ok(Some(js_reply)) => ($with_reply)(js_reply),
        _ => $reply.error(EIO),
      }
    }
  };
  ($js_fn:expr, $args:expr, $out_type:ty, $reply:ident, @napi-thread => $with_reply:expr) => {
    $js_fn.call_with_return_value(
      Ok($args.into()),
      ThreadsafeFunctionCallMode::Blocking,
      move |js_reply, env| {
        match js_reply {
          Ok(js_reply) => {
            let _ = env.spawn_future(async move {
              match js_reply.await {
                Ok(js_reply) => ($with_reply)(js_reply),
                Err(_) => $reply.error(EIO),
              };
              Ok(())
            });
          },
          Err(_) => $reply.error(EIO)
        };
        Ok(())
      }
    );
  };
}

const EIO: c_int = 5;

fn to_opt_i64(x: Option<u64>) -> Option<i64> {
  match x { Some(n) => Some(n as i64), _ => None }
}
fn str_from_os(s: &OsStr) -> String {
  s.to_str().unwrap().to_string()
}

macro_rules! send_err {
  ($code:expr, $reply:ident) => {
    {
      $reply.error($code);
    }
  }
}

fn send_xattr(xattr: XAttrBytesOrErr, reply: ReplyXattr) {
  match xattr {
    XAttrBytesOrErr::Data(data) => reply.data(&data),
    XAttrBytesOrErr::Size(size) => reply.size(size),
    XAttrBytesOrErr::Err(code) => send_err!(code, reply),
  };
}

const TTL: Duration = Duration::from_secs(1);

impl Filesystem for CallbacksProxy {

  fn init(&mut self, _req: &Request<'_>, _config: &mut KernelConfig) -> Result<(), c_int> {
    call_js!(self.cbs.init, (FUSE_ROOT_ID as i64));
    Ok(())
  }

  fn destroy(&mut self) {
    call_js!(self.cbs.destroy);
  }

  fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
    call_js!(
      self.cbs.lookup, (parent as i64, str_from_os(name)), FileAttrOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          FileAttrOrErr::Attr(attrs) => reply.entry(&TTL, &attrs.into_fuse(), 0),
          FileAttrOrErr::Err(code) => send_err!(code, reply),
        }
      }
    );
  }

  fn forget(&mut self, _req: &Request<'_>, ino: u64, nlookup: u64) {
    call_js!(self.cbs.forget, (ino as i64, nlookup as i64));
  }

  fn batch_forget(&mut self, req: &Request<'_>, nodes: &[fuse_forget_one]) {
    for node in nodes {
      self.forget(req, node.nodeid, node.nlookup);
    }
  }

  fn getattr(&mut self, _req: &Request<'_>, ino: u64, fh: Option<u64>, reply: ReplyAttr) {
    call_js!(
      self.cbs.getattr, (ino as i64, to_opt_i64(fh)), FileAttrOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          FileAttrOrErr::Attr(attrs) => reply.attr(&TTL, &attrs.into_fuse()),
          FileAttrOrErr::Err(code) => send_err!(code, reply),
        }
      }
    );
  }

  fn setattr(
    &mut self,
    _req: &Request<'_>,
    ino: u64,
    mode: Option<u32>,
    uid: Option<u32>,
    gid: Option<u32>,
    _size: Option<u64>,
    _atime: Option<TimeOrNow>,
    _mtime: Option<TimeOrNow>,
    _ctime: Option<SystemTime>,
    fh: Option<u64>,
    _crtime: Option<SystemTime>,
    _chgtime: Option<SystemTime>,
    _bkuptime: Option<SystemTime>,
    flags: Option<u32>,
    reply: ReplyAttr,
  ) {
    let changes = AttrChanges { mode, uid, gid, flags };
    call_js!(
      self.cbs.setattr, (ino as i64, to_opt_i64(fh), changes), FileAttrOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          FileAttrOrErr::Attr(attrs) => reply.attr(&TTL, &attrs.into_fuse()),
          FileAttrOrErr::Err(code) => send_err!(code, reply),
        }
      }
    );
  }

  fn readlink(&mut self, _req: &Request<'_>, ino: u64, reply: ReplyData) {
    send_err!(ENOSYS, reply);
  }

  // fn mknod(
  //   &mut self,
  //   _req: &Request<'_>,
  //   parent: u64,
  //   name: &OsStr,
  //   mode: u32,
  //   umask: u32,
  //   rdev: u32,
  //   reply: ReplyEntry,
  // ) {
  //   let name_str = name.display().to_string();
  //   js_call!(self.cbs.test, "mknod", {
  //     println!("🧐 fuser.mknod(parent: {parent:#x?}, name: {name_str:?}, \
  //       mode: {mode}, umask: {umask:#x?}, rdev: {rdev})"
  //     );
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn mkdir(
  //   &mut self,
  //   _req: &Request<'_>,
  //   parent: u64,
  //   name: &OsStr,
  //   mode: u32,
  //   umask: u32,
  //   reply: ReplyEntry,
  // ) {
  //   let name_str = name.display().to_string();
  //   js_call!(self.cbs.test, "mkdir", {
  //     println!("🧐 fuser.mkdir(parent: {parent:#x?}, name: {name_str:?}, mode: {mode}, umask: {umask:#x?})");
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn unlink(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
  //   let name_str = name.display().to_string();
  //   js_call!(self.cbs.test, "unlink", {
  //     println!("🧐 fuser.unlink(parent: {parent:#x?}, name: {name_str:?})",);
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn rmdir(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
  //   let name_str = name.display().to_string();
  //   js_call!(self.cbs.test, "rmdir", {
  //     println!("🧐 fuser.rmdir(parent: {parent:#x?}, name: {name_str:?})",);
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn symlink(
  //   &mut self,
  //   _req: &Request<'_>,
  //   parent: u64,
  //   link_name: &OsStr,
  //   target: &Path,
  //   reply: ReplyEntry,
  // ) {
  //   let link_name_str = link_name.display().to_string();
  //   let target_str = target.display().to_string();
  //   js_call!(self.cbs.test, "symlink", {
  //     println!("🧐 fuser.symlink(parent: {parent:#x?}, link_name: {link_name_str:?}, target: {target_str:?})");
  //     send_err!(EPERM);
  //   });
  // }

  // fn rename(
  //   &mut self,
  //   _req: &Request<'_>,
  //   parent: u64,
  //   name: &OsStr,
  //   newparent: u64,
  //   newname: &OsStr,
  //   flags: u32,
  //   reply: ReplyEmpty,
  // ) {
  //   let name_str = name.display().to_string();
  //   let newname_str = newname.display().to_string();
  //   js_call!(self.cbs.test, "rename", {
  //     println!("🧐 fuser.rename(parent: {parent:#x?}, name: {name_str:?}, newparent: {newparent:#x?}, newname: {newname_str:?}, flags: {flags})");
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn link(
  //   &mut self,
  //   _req: &Request<'_>,
  //   ino: u64,
  //   newparent: u64,
  //   newname: &OsStr,
  //   reply: ReplyEntry,
  // ) {
  //   let newname_str = newname.display().to_string();
  //   js_call!(self.cbs.test, "link", {
  //     println!("🧐 fuser.link(ino: {ino:#x?}, newparent: {newparent:#x?}, newname: {newname_str:?})");
  //     send_err!(EPERM);
  //   });
  // }

  fn open(&mut self, _req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
    call_js!(
      self.cbs.open, (ino as i64, flags), ParamsOfOpenedOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          ParamsOfOpenedOrErr::Params(params) => reply.opened(params.fh as u64, params.flags),
          ParamsOfOpenedOrErr::Err(code) => send_err!(code, reply),
        }
      }
    );
  }

  fn read(
    &mut self,
    _req: &Request<'_>,
    ino: u64,
    fh: u64,
    offset: i64,
    size: u32,
    flags: i32,
    lock_owner: Option<u64>,
    reply: ReplyData,
  ) {
    let args = ReadArgs { offset, size, flags, lock_owner: to_opt_i64(lock_owner) };
    call_js!(
      self.cbs.read, (ino as i64, fh as i64, args), BufferOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          BufferOrErr::Ok(data) => reply.data(&data),
          BufferOrErr::Err(code) => send_err!(code, reply),
        }
      }
    );
  }

  // fn write(
  //   &mut self,
  //   _req: &Request<'_>,
  //   ino: u64,
  //   fh: u64,
  //   offset: i64,
  //   data: &[u8],
  //   write_flags: u32,
  //   flags: i32,
  //   lock_owner: Option<u64>,
  //   reply: ReplyWrite,
  // ) {
  //   let data_len = data.len();
  //   js_call!(self.cbs.test, "write", {
  //     println!("🧐 fuser.write(ino: {ino:#x?}, fh: {fh}, offset: {offset}, data.len(): {}, write_flags: {write_flags:#x?}, flags: {flags:#x?}, lock_owner: {lock_owner:?})", data_len);
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn flush(&mut self, _req: &Request<'_>, ino: u64, fh: u64, lock_owner: u64, reply: ReplyEmpty) {
  //   js_call!(self.cbs.test, "flush", {
  //     println!("🧐 fuser.flush(ino: {ino:#x?}, fh: {fh}, lock_owner: {lock_owner:?})");
  //     send_err!(ENOSYS);
  //   });
  // }

  fn release(
    &mut self,
    _req: &Request<'_>,
    ino: u64,
    fh: u64,
    flags: i32,
    lock_owner: Option<u64>,
    flush: bool,
    reply: ReplyEmpty,
  ) {
    let args = ReleaseArgs { flags, flush, lock_owner: to_opt_i64(lock_owner) };
    call_js!(
      self.cbs.release, (ino as i64, fh as i64, args), (), reply,
      @initial-thread => |_| { reply.ok(); }
    );
  }

  // fn fsync(&mut self, _req: &Request<'_>, ino: u64, fh: u64, datasync: bool, reply: ReplyEmpty) {
  //   js_call!(self.cbs.test, "fsync", {
  //     println!("🧐 fuser.fsync(ino: {ino:#x?}, fh: {fh}, datasync: {datasync})");
  //     send_err!(ENOSYS);
  //   });
  // }

  fn opendir(&mut self, _req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
    call_js!(
      self.cbs.opendir, (ino as i64, flags), ParamsOfOpenedOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          ParamsOfOpenedOrErr::Params(params) => reply.opened(params.fh as u64, params.flags),
          ParamsOfOpenedOrErr::Err(code) => send_err!(code, reply),
        }
      }
    );
  }

  fn readdir(
    &mut self,
    _req: &Request<'_>,
    ino: u64,
    fh: u64,
    offset: i64,
    mut reply: ReplyDirectory,
  ) {
    call_js!(
      self.cbs.readdir, (ino as i64, fh as i64, offset), DirListing, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          DirListing::Lst(lst) => {
            for entry in lst {
              let buffer_full = reply.add(ino, entry.offset, to_file_type(&entry.kind), &&OsStr::new(&entry.name));
              if buffer_full {
                break;
              }
            }
            reply.ok();
          },
          DirListing::Err(code) => send_err!(code, reply),
        }
      }
    );
  }

  // fn readdirplus(
  //   &mut self,
  //   _req: &Request<'_>,
  //   ino: u64,
  //   fh: u64,
  //   offset: i64,
  //   reply: ReplyDirectoryPlus,
  // ) {
  //   js_call!(self.cbs.test, "readdirplus", {
  //     println!("🧐 fuser.readdirplus(ino: {ino:#x?}, fh: {fh}, offset: {offset})");
  //     send_err!(ENOSYS);
  //   });
  // }

  fn releasedir(
    &mut self,
    _req: &Request<'_>,
    ino: u64,
    fh: u64,
    flags: i32,
    reply: ReplyEmpty,
  ) {
    call_js!(
      self.cbs.releasedir, (ino as i64, fh as i64, flags), (), reply,
      @initial-thread => |_| { reply.ok(); }
    );
  }

  // fn fsyncdir(
  //   &mut self,
  //   _req: &Request<'_>,
  //   ino: u64,
  //   fh: u64,
  //   datasync: bool,
  //   reply: ReplyEmpty,
  // ) {
  //   js_call!(self.cbs.test, "fsyncdir", {
  //     println!("🧐 fuser.fsyncdir(ino: {ino:#x?}, fh: {fh}, datasync: {datasync})");
  //     send_err!(ENOSYS);
  //   });
  // }

  fn statfs(&mut self, _req: &Request<'_>, _ino: u64, reply: ReplyStatfs) {
    reply.statfs(0, 0, 0, 0, 0, BLOCK_SIZE as u32, 255, BLOCK_SIZE as u32);
  }

  // fn setxattr(
  //   &mut self,
  //   _req: &Request<'_>,
  //   ino: u64,
  //   name: &OsStr,
  //   _value: &[u8],
  //   flags: i32,
  //   position: u32,
  //   reply: ReplyEmpty,
  // ) {
  //   let name_str = name.display().to_string();
  //   js_call!(self.cbs.test, "setxattr", {
  //     println!("🧐 fuser.setxattr(ino: {ino:#x?}, name: {name_str:?}, flags: {flags:#x?}, position: {position})");
  //     send_err!(ENOSYS);
  //   });
  // }

  fn getxattr(
    &mut self,
    _req: &Request<'_>,
    ino: u64,
    name: &OsStr,
    size: u32,
    reply: ReplyXattr,
  ) {
    call_js!(
      self.cbs.getxattr, (ino as i64, str_from_os(name), size), XAttrBytesOrErr, reply,
      @initial-thread => |js_reply| { send_xattr(js_reply, reply); }
    );
  }

  fn listxattr(&mut self, _req: &Request<'_>, ino: u64, size: u32, reply: ReplyXattr) {
    call_js!(
      self.cbs.listxattr, (ino as i64, size), XAttrBytesOrErr, reply,
      @initial-thread => |js_reply| { send_xattr(js_reply, reply); }
    );
  }

  // fn removexattr(&mut self, _req: &Request<'_>, ino: u64, name: &OsStr, reply: ReplyEmpty) {
  //   let name_str = name.display().to_string();
  //   js_call!(self.cbs.test, "removexattr", {
  //     println!("🧐 fuser.removexattr(ino: {ino:#x?}, name: {name_str:?})");
  //     send_err!(ENOSYS);
  //   });
  // }

  fn access(&mut self, _req: &Request<'_>, ino: u64, mask: i32, reply: ReplyEmpty) {
    call_js!(
      self.cbs.access, (ino as i64, mask), i32, reply,
      @initial-thread => |err_code| {
        if err_code == 0 {
          reply.ok();
        } else {
          send_err!(err_code, reply);
        }
      }
    );
  }

  // fn create(
  //   &mut self,
  //   _req: &Request<'_>,
  //   parent: u64,
  //   name: &OsStr,
  //   mode: u32,
  //   umask: u32,
  //   flags: i32,
  //   reply: ReplyCreate,
  // ) {
  //   let name_str = name.display().to_string();
  //   js_call!(self.cbs.test, "create", {
  //     println!("🧐 fuser.create(parent: {parent:#x?}, name: {name_str:?}, mode: {mode}, umask: {umask:#x?}, flags: {flags:#x?})");
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn getlk(
  //   &mut self,
  //   _req: &Request<'_>,
  //   ino: u64,
  //   fh: u64,
  //   lock_owner: u64,
  //   start: u64,
  //   end: u64,
  //   typ: i32,
  //   pid: u32,
  //   reply: ReplyLock,
  // ) {
  //   js_call!(self.cbs.test, "getlk", {
  //     println!("🧐 fuser.getlk(ino: {ino:#x?}, fh: {fh}, lock_owner: {lock_owner}, start: {start}, end: {end}, typ: {typ}, pid: {pid})");
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn setlk(
  //   &mut self,
  //   _req: &Request<'_>,
  //   ino: u64,
  //   fh: u64,
  //   lock_owner: u64,
  //   start: u64,
  //   end: u64,
  //   typ: i32,
  //   pid: u32,
  //   sleep: bool,
  //   reply: ReplyEmpty,
  // ) {
  //   js_call!(self.cbs.test, "setlk", {
  //     println!("🧐 fuser.setlk(ino: {ino:#x?}, fh: {fh}, lock_owner: {lock_owner}, start: {start}, end: {end}, typ: {typ}, pid: {pid}, sleep: {sleep})");
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn bmap(&mut self, _req: &Request<'_>, ino: u64, blocksize: u32, idx: u64, reply: ReplyBmap) {
  //   js_call!(self.cbs.test, "bmap", {
  //     println!("🧐 fuser.bmap(ino: {ino:#x?}, blocksize: {blocksize}, idx: {idx})",);
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn ioctl(
  //   &mut self,
  //   _req: &Request<'_>,
  //   ino: u64,
  //   fh: u64,
  //   flags: u32,
  //   cmd: u32,
  //   in_data: &[u8],
  //   out_size: u32,
  //   reply: ReplyIoctl,
  // ) {
  //   let in_data_len = in_data.len();
  //   js_call!(self.cbs.test, "bmap", {
  //     println!("🧐 fuser.ioctl(ino: {ino:#x?}, fh: {fh}, flags: {flags}, cmd: {cmd}, in_data.len(): {}, out_size: {out_size})", in_data_len);
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn poll(
  //   &mut self,
  //   _req: &Request<'_>,
  //   ino: u64,
  //   fh: u64,
  //   ph: PollHandle,
  //   events: u32,
  //   flags: u32,
  //   reply: ReplyPoll,
  // ) {
  //   js_call!(self.cbs.test, "poll", {
  //     println!("🧐 fuser.poll(ino: {ino:#x?}, fh: {fh}, ph: {ph:?}, events: {events}, flags: {flags})");
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn fallocate(
  //   &mut self,
  //   _req: &Request<'_>,
  //   ino: u64,
  //   fh: u64,
  //   offset: i64,
  //   length: i64,
  //   mode: i32,
  //   reply: ReplyEmpty,
  // ) {
  //   js_call!(self.cbs.test, "fallocate", {
  //     println!("🧐 fuser.fallocate(ino: {ino:#x?}, fh: {fh}, offset: {offset}, length: {length}, mode: {mode})");
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn lseek(
  //   &mut self,
  //   _req: &Request<'_>,
  //   ino: u64,
  //   fh: u64,
  //   offset: i64,
  //   whence: i32,
  //   reply: ReplyLseek,
  // ) {
  //   js_call!(self.cbs.test, "lseek", {
  //     println!("🧐 fuser.lseek(ino: {ino:#x?}, fh: {fh}, offset: {offset}, whence: {whence})");
  //     send_err!(ENOSYS);
  //   });
  // }

  // fn copy_file_range(
  //   &mut self,
  //   _req: &Request<'_>,
  //   ino_in: u64,
  //   fh_in: u64,
  //   offset_in: i64,
  //   ino_out: u64,
  //   fh_out: u64,
  //   offset_out: i64,
  //   len: u64,
  //   flags: u32,
  //   reply: ReplyWrite,
  // ) {
  //   js_call!(self.cbs.test, "copy_file_range", {
  //     println!("🧐 fuser.copy_file_range(ino_in: {ino_in:#x?}, fh_in: {fh_in}, offset_in: {offset_in}, ino_out: {ino_out:#x?}, fh_out: {fh_out}, offset_out: {offset_out}, len: {len}, flags: {flags})");
  //     send_err!(ENOSYS);
  //   });
  // }

  // #[cfg(target_os = "macos")]
  // fn setvolname(&mut self, _req: &Request<'_>, name: &OsStr, reply: ReplyEmpty) {
  //   let name_str = name.display().to_string();
  //   js_call!(self.cbs.test, "copy_file_range", {
  //     println!("🧐 fuser.setvolname(name: {name_str:?})");
  //     send_err!(ENOSYS);
  //   });
  // }
}
