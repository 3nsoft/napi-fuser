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

use std::{ffi::OsStr, io, path::Path, sync::mpsc::channel, time::{Duration, SystemTime}};

use fuser::{AccessFlags, BsdFileFlags, Errno, FileHandle, Filesystem, FopenFlags, Generation, INodeNo, KernelConfig, LockOwner, OpenFlags, RenameFlags, ReplyAttr, ReplyBmap, ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyLock, ReplyOpen, ReplyStatfs, ReplyXattr, Request, TimeOrNow};
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
        _ => $reply.error(Errno::EIO),
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
                Err(_) => $reply.error(Errno::EIO),
              };
              Ok(())
            });
          },
          Err(_) => $reply.error(Errno::EIO)
        };
        Ok(())
      }
    );
  };
}

fn fh_opt_i64(x: Option<FileHandle>) -> Option<i64> {
  match x { Some(n) => Some(n.0 as i64), _ => None }
}
fn lo_opt_i64(x: Option<LockOwner>) -> Option<i64> {
  match x { Some(n) => Some(n.0 as i64), _ => None }
}
fn str_from_os(s: &OsStr) -> String {
  s.to_str().unwrap().to_string()
}
fn to_opt_u32(x: Option<BsdFileFlags>) -> Option<u32> {
  match x { Some(n) => Some(n.bits()), _ => None }
}

fn send_xattr(xattr: XAttrBytesOrErr, reply: ReplyXattr) {
  match xattr {
    XAttrBytesOrErr::Data(data) => reply.data(&data),
    XAttrBytesOrErr::Size(size) => reply.size(size),
    XAttrBytesOrErr::Err(code) => reply.error(Errno::from_i32(code)),
  };
}

fn send_empty(err_code: i32, reply: ReplyEmpty) {
  if err_code == 0 {
    reply.ok();
  } else {
    reply.error(Errno::from_i32(err_code));
  }
}

const TTL: Duration = Duration::from_secs(1);

impl Filesystem for CallbacksProxy {

  fn init(&mut self, _req: &Request, _config: &mut KernelConfig) -> io::Result<()> {
    call_js!(self.cbs.init, (INodeNo::ROOT.0 as i64));
    Ok(())
  }

  fn destroy(&mut self) {
    call_js!(self.cbs.destroy);
  }

  fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
    call_js!(
      self.cbs.lookup, (parent.0 as i64, str_from_os(name)), FileAttrOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          FileAttrOrErr::Attr(attrs) => reply.entry(&TTL, &attrs.into_fuse(), Generation(0)),
          FileAttrOrErr::Err(code) => reply.error(Errno::from_i32(code)),
        }
      }
    );
  }

  fn forget(&self, _req: &Request, ino: INodeNo, nlookup: u64) {
    call_js!(self.cbs.forget, (ino.0 as i64, nlookup as i64));
  }

  fn getattr(&self, _req: &Request, ino: INodeNo, fh: Option<FileHandle>, reply: ReplyAttr) {
    call_js!(
      self.cbs.getattr, (ino.0 as i64, fh_opt_i64(fh)), FileAttrOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          FileAttrOrErr::Attr(attrs) => reply.attr(&TTL, &attrs.into_fuse()),
          FileAttrOrErr::Err(code) => reply.error(Errno::from_i32(code)),
        }
      }
    );
  }

  fn setattr(
    &self,
    _req: &Request,
    ino: INodeNo,
    mode: Option<u32>,
    uid: Option<u32>,
    gid: Option<u32>,
    _size: Option<u64>,
    _atime: Option<TimeOrNow>,
    _mtime: Option<TimeOrNow>,
    _ctime: Option<SystemTime>,
    fh: Option<FileHandle>,
    _crtime: Option<SystemTime>,
    _chgtime: Option<SystemTime>,
    _bkuptime: Option<SystemTime>,
    flags: Option<BsdFileFlags>,
    reply: ReplyAttr,
  ) {
    let changes = AttrChanges { mode, uid, gid, flags: to_opt_u32(flags) };
    call_js!(
      self.cbs.setattr, (ino.0 as i64, fh_opt_i64(fh), changes), FileAttrOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          FileAttrOrErr::Attr(attrs) => reply.attr(&TTL, &attrs.into_fuse()),
          FileAttrOrErr::Err(code) => reply.error(Errno::from_i32(code)),
        }
      }
    );
  }

  fn readlink(&self, _req: &Request, _ino: INodeNo, reply: ReplyData) {
    reply.error(Errno::ENOSYS);
  }

  fn mknod(
    &self, _req: &Request, parent: INodeNo, name: &OsStr, mode: u32, umask: u32, rdev: u32, reply: ReplyEntry
  ) {
    let name_str = name.display().to_string();
    call_js!(
      self.cbs.mknod, (parent.0 as i64, name_str, mode, umask, rdev), NewEntryOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          NewEntryOrErr::Entry(r) => reply.entry(
            &Duration::from_millis(r.ttl as u64), &r.attr.into_fuse(), Generation(r.generation as u64)
          ),
          NewEntryOrErr::Err(code) => reply.error(Errno::from_i32(code)),
        }
      }
    )
  }

  fn mkdir(
    &self, _req: &Request, parent: INodeNo, name: &OsStr, mode: u32, umask: u32, reply: ReplyEntry
  ) {
    let name_str = name.display().to_string();
    call_js!(
      self.cbs.mkdir, (parent.0 as i64, name_str, mode, umask), NewEntryOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          NewEntryOrErr::Entry(r) => reply.entry(
            &Duration::from_millis(r.ttl as u64), &r.attr.into_fuse(), Generation(r.generation as u64)
          ),
          NewEntryOrErr::Err(code) => reply.error(Errno::from_i32(code)),
        }
      }
    )
  }

  fn unlink(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
    let name_str = name.display().to_string();
    call_js!(
      self.cbs.unlink, (parent.0 as i64, name_str), i32, reply,
      @initial-thread => |err_code| { send_empty(err_code, reply); }
    );
  }

  fn rmdir(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
    let name_str = name.display().to_string();
    call_js!(
      self.cbs.rmdir, (parent.0 as i64, name_str), i32, reply,
      @initial-thread => |err_code| { send_empty(err_code, reply); }
    );
  }

  /// We don't do symbolic linking.
  fn symlink(&self, _req: &Request, _parent: INodeNo, _link_name: &OsStr, _target: &Path, reply: ReplyEntry) {
    reply.error(Errno::EPERM);
  }

  fn rename(
    &self, _req: &Request, parent: INodeNo, name: &OsStr, newparent: INodeNo, newname: &OsStr,
    flags: RenameFlags, reply: ReplyEmpty,
  ) {
    let name_str = name.display().to_string();
    let newname_str = newname.display().to_string();
    call_js!(
      self.cbs.rename, (parent.0 as i64, name_str, newparent.0 as i64, newname_str, flags.bits()), i32, reply,
      @initial-thread => |err_code| { send_empty(err_code, reply); }
    );
  }

  /// We don't do linking.
  fn link(&self, _req: &Request, _ino: INodeNo, _newparent: INodeNo, _newname: &OsStr, reply: ReplyEntry) {
    reply.error(Errno::EPERM);
  }

  fn open(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
    call_js!(
      self.cbs.open, (ino.0 as i64, flags.0), ParamsOfOpenedOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          ParamsOfOpenedOrErr::Params(params) => match FopenFlags::from_bits(params.flags) {
            Some(flags) => reply.opened(FileHandle(params.fh as u64), flags),
            None => reply.error(Errno::EIO)
          },
          ParamsOfOpenedOrErr::Err(code) => reply.error(Errno::from_i32(code)),
        }
      }
    );
  }

  fn read(
    &self, _req: &Request, ino: INodeNo, fh: FileHandle, offset: u64, size: u32, flags: OpenFlags,
    lock_owner: Option<LockOwner>, reply: ReplyData,
  ) {
    let args = ReadArgs {
      offset: offset as i64,
      size,
      flags: flags.0,
      lock_owner: lo_opt_i64(lock_owner)
    };
    call_js!(
      self.cbs.read, (ino.0 as i64, fh.0 as i64, args), BufferOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          BufferOrErr::Ok(data) => reply.data(&data),
          BufferOrErr::Err(code) => reply.error(Errno::from_i32(code)),
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

  fn flush(&self, _req: &Request, ino: INodeNo, fh: FileHandle, lock_owner: LockOwner, reply: ReplyEmpty) {
    call_js!(
      self.cbs.flush, (ino.0 as i64, fh.0 as i64, lock_owner.0 as i64), i32, reply,
      @initial-thread => |err_code| { send_empty(err_code, reply); }
    );
  }

  fn release(
    &self, _req: &Request, ino: INodeNo, fh: FileHandle, flags: OpenFlags,
    lock_owner: Option<LockOwner>, flush: bool, reply: ReplyEmpty,
  ) {
    let args = ReleaseArgs {
      flags: flags.0, flush, lock_owner: lo_opt_i64(lock_owner)
    };
    call_js!(
      self.cbs.release, (ino.0 as i64, fh.0 as i64, args), i32, reply,
      @initial-thread => |err_code| { send_empty(err_code, reply); }
    );
  }

  fn fsync(&self, _req: &Request, ino: INodeNo, fh: FileHandle, datasync: bool, reply: ReplyEmpty) {
    call_js!(
      self.cbs.fsync, (ino.0 as i64, fh.0 as i64, datasync), i32, reply,
      @initial-thread => |err_code| { send_empty(err_code, reply); }
    );
  }

  fn opendir(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
    call_js!(
      self.cbs.opendir, (ino.0 as i64, flags.0), ParamsOfOpenedOrErr, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          ParamsOfOpenedOrErr::Params(params) => match FopenFlags::from_bits(params.flags) {
            Some(flags) => reply.opened(FileHandle(params.fh as u64), flags),
            None => reply.error(Errno::EIO)
          }
          ParamsOfOpenedOrErr::Err(code) => reply.error(Errno::from_i32(code)),
        }
      }
    );
  }

  fn readdir(&self, _req: &Request, ino: INodeNo, fh: FileHandle, offset: u64, mut reply: ReplyDirectory) {
    call_js!(
      self.cbs.readdir, (ino.0 as i64, fh.0 as i64, offset as i64), DirListing, reply,
      @initial-thread => |js_reply| {
        match js_reply {
          DirListing::Lst(lst) => {
            for entry in lst {
              let buffer_full = reply.add(
                INodeNo(entry.ino as u64), entry.offset as u64, to_file_type(&entry.kind), OsStr::new(&entry.name)
              );
              if buffer_full {
                break;
              }
            }
            reply.ok();
          },
          DirListing::Err(code) => reply.error(Errno::from_i32(code)),
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
    &self,
    _req: &Request,
    ino: INodeNo,
    fh: FileHandle,
    flags: OpenFlags,
    reply: ReplyEmpty,
  ) {
    call_js!(
      self.cbs.releasedir, (ino.0 as i64, fh.0 as i64, flags.0), i32, reply,
      @initial-thread => |err_code| { send_empty(err_code, reply); }
    );
  }

  fn fsyncdir(&self, _req: &Request, ino: INodeNo, fh: FileHandle, datasync: bool, reply: ReplyEmpty) {
    call_js!(
      self.cbs.fsyncdir, (ino.0 as i64, fh.0 as i64, datasync), i32, reply,
      @initial-thread => |err_code| { send_empty(err_code, reply); }
    );
  }

  fn statfs(&self, _req: &Request, _ino: INodeNo, reply: ReplyStatfs) {
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

  fn getxattr(&self, _req: &Request, ino: INodeNo, name: &OsStr, size: u32, reply: ReplyXattr) {
    call_js!(
      self.cbs.getxattr, (ino.0 as i64, str_from_os(name), size), XAttrBytesOrErr, reply,
      @initial-thread => |js_reply| { send_xattr(js_reply, reply); }
    );
  }

  fn listxattr(&self, _req: &Request, ino: INodeNo, size: u32, reply: ReplyXattr) {
    call_js!(
      self.cbs.listxattr, (ino.0 as i64, size), XAttrBytesOrErr, reply,
      @initial-thread => |js_reply| { send_xattr(js_reply, reply); }
    );
  }

  fn removexattr(&self, _req: &Request, ino: INodeNo, name: &OsStr, reply: ReplyEmpty) {
    let name_str = name.display().to_string();
    call_js!(
      self.cbs.removexattr, (ino.0 as i64, name_str), i32, reply,
      @initial-thread => |js_reply| { send_empty(js_reply, reply); }
    );
  }

  fn access(&self, _req: &Request, ino: INodeNo, mask: AccessFlags, reply: ReplyEmpty) {
    call_js!(
      self.cbs.access, (ino.0 as i64, mask.bits()), i32, reply,
      @initial-thread => |err_code| { send_empty(err_code, reply); }
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

  fn getlk(
    &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _lock_owner: LockOwner,
    _start: u64, _end: u64, _typ: i32, _pid: u32, reply: ReplyLock,
  ) {
    reply.error(Errno::ENOSYS);
  }

  fn setlk(
    &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _lock_owner: LockOwner,
    _start: u64, _end: u64, _typ: i32, _pid: u32, _sleep: bool, reply: ReplyEmpty,
  ) {
    reply.error(Errno::ENOSYS);
  }

  fn bmap(&self, _req: &Request, _ino: INodeNo, _blocksize: u32, _idx: u64, reply: ReplyBmap) {
    reply.error(Errno::ENOSYS);
  }

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

  #[cfg(target_os = "macos")]
  fn setvolname(&self, _req: &Request, _name: &OsStr, reply: ReplyEmpty) {
    reply.error(Errno::EPERM);
  }
}
