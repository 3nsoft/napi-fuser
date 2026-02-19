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

use std::time::{Duration, SystemTime};

use fuser::{FileType, INodeNo};
use napi::{bindgen_prelude::{Buffer, FnArgs, Promise}, threadsafe_function::ThreadsafeFunction};
use napi_derive::napi;

/// init [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust crate.
#[napi]
pub type InitOpCB = ThreadsafeFunction<i64>;

/// destory [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
#[napi]
pub type DestroyOpCB = ThreadsafeFunction<()>;

/// lookup [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
/// 
/// Arguments:
/// 1. usual NAPI error. No fs-level error expected here from this module.
/// 2. ino of parent directory, in which lookup is done to see child with given name.
/// 3. child name, which attributes FUSE is requesting.
/// 
/// Should return filesystem error code or an attributes data.
#[napi]
pub type LookupOpCB = ThreadsafeFunction<FnArgs<(i64, String)>, Promise<FileAttrOrErr>>;

/// forget [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
/// 
/// Arguments:
/// 1. ino
/// 2. nlookup - count of lookups to drop.
#[napi]
pub type ForgetOpCB = ThreadsafeFunction<FnArgs<(i64, i64)>>;

/// getattr [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
/// 
/// Arguments:
/// 1. ino
/// 2. fh
/// 
/// Should return filesystem error code or an attributes data.
#[napi]
pub type GetAttrOpCB = ThreadsafeFunction<FnArgs<(i64, Option<i64>)>, Promise<FileAttrOrErr>>;

/// setattr [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
/// 
/// Arguments:
/// 1. ino
/// 2. fh
/// 
/// Should return filesystem error code or updated attributes data.
#[napi]
pub type SetAttrOpCB = ThreadsafeFunction<FnArgs<(i64, Option<i64>, AttrChanges)>, Promise<FileAttrOrErr>>;

/// mknod [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
/// 
/// Arguments:
/// 1. parent inode id
/// 2. name of a new child node
/// 3. mode
/// 4. umask
/// 5. rdev id
#[napi]
pub type MkNodOpCB = ThreadsafeFunction<FnArgs<(i64, String, u32, u32, u32)>, Promise<NewEntryOrErr>>;

/// mkdir [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
/// 
/// Arguments:
/// 1. parent inode id
/// 2. name of a new child
/// 3. mode
/// 4. umask
#[napi]
pub type MkDirOpCB = ThreadsafeFunction<FnArgs<(i64, String, u32, u32)>, Promise<NewEntryOrErr>>;

/// unlink [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
/// 
/// Arguments:
/// 1. parent inode id
/// 2. name of a child to remove
#[napi]
pub type UnlinkOpCB = ThreadsafeFunction<FnArgs<(i64, String)>, Promise<i32>>;

/// rmdir [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
/// 
/// Arguments:
/// 1. parent inode id
/// 2. name of a child folder to remove
#[napi]
pub type RmDirOpCB = ThreadsafeFunction<FnArgs<(i64, String)>, Promise<i32>>;

/// rename [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
/// 
/// Arguments:
/// 1. parent inode id
/// 2. name of a child to move
/// 3. newparent inode id
/// 4. newname of a child in new parent
#[napi]
pub type RenameOpCB = ThreadsafeFunction<FnArgs<(i64, String, i64, String, u32)>, Promise<i32>>;

/// open [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust crate.
#[napi]
pub type OpenOpCB = ThreadsafeFunction<FnArgs<(i64, i32)>, Promise<ParamsOfOpenedOrErr>>;

/// read [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust crate.
#[napi]
pub type ReadOpCB = ThreadsafeFunction<FnArgs<(i64, i64, ReadArgs)>, Promise<BufferOrErr>>;

/// release [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
#[napi]
pub type ReleaseOpCB = ThreadsafeFunction<FnArgs<(i64, i64, ReleaseArgs)>, Promise<i32>>;

/// flush [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
/// 
/// Arguments:
/// 1. ino
/// 2. fh
/// 3. lock_owner
#[napi]
pub type FlushOpCB = ThreadsafeFunction<FnArgs<(i64, i64, i64)>, Promise<i32>>;

/// fsync [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
/// 
/// Arguments:
/// 1. ino
/// 2. fh
/// 3. datasync flag
#[napi]
pub type FSyncOpCB = ThreadsafeFunction<FnArgs<(i64, i64, bool)>, Promise<i32>>;

/// opendir [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
#[napi]
pub type OpenDirOpCB = ThreadsafeFunction<FnArgs<(i64, i32)>, Promise<ParamsOfOpenedOrErr>>;

/// readdir [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
#[napi]
pub type ReadDirOpCB = ThreadsafeFunction<FnArgs<(i64, i64, i64)>, Promise<DirListing>>;

/// releasedir [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
#[napi]
pub type ReleaseDirOpCB = ThreadsafeFunction<FnArgs<(i64, i64, i32)>, Promise<i32>>;

/// fsyncdir [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
/// 
/// Arguments:
/// 1. ino
/// 2. fh
/// 3. datasync flag
#[napi]
pub type FSyncDirOpCB = ThreadsafeFunction<FnArgs<(i64, i64, bool)>, Promise<i32>>;

/// getxattr [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
#[napi]
pub type GetXAttrOpCB = ThreadsafeFunction<FnArgs<(i64, String, u32)>, Promise<XAttrBytesOrErr>>;

/// listxattr [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
#[napi]
pub type ListXAttrOpCB = ThreadsafeFunction<FnArgs<(i64, u32)>, Promise<XAttrBytesOrErr>>;

/// removexattr [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust
/// crate.
/// 
/// Arguments:
/// 1. ino
/// 2. name of xattr to remove
#[napi]
pub type RemoveXAttrOpCB = ThreadsafeFunction<FnArgs<(i64, String)>, Promise<i32>>;

/// access [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html) using fuser Rust crate.
#[napi]
pub type AccessOpCB = ThreadsafeFunction<FnArgs<(i64, i32)>, Promise<i32>>;

/// This contains JavaScript callbacks to perform
/// [FUSE operation](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html), structured by [`fuser`].
/// 
/// NAPI-RS' [`ThreadsafeFunction`] is used for callbacks, as [`fuser`] calls implementation in its own thread.
/// 
/// If operation has too many arguments besides inode and file handler, these are packed into respective
/// structs/napi-objects.
pub struct CallbacksToJS {
  pub init: InitOpCB,
  pub destroy: DestroyOpCB,
  pub lookup: LookupOpCB,
  pub forget: ForgetOpCB,
  pub getattr: GetAttrOpCB,
  pub setattr: SetAttrOpCB,
  pub mknod: MkNodOpCB,
  pub mkdir: MkDirOpCB,
  pub unlink: UnlinkOpCB,
  pub rmdir: RmDirOpCB,
  pub rename: RenameOpCB,
  pub open: OpenOpCB,
  pub read: ReadOpCB,
  pub flush: FlushOpCB,
  pub release: ReleaseOpCB,
  pub fsync: FSyncOpCB,
  pub opendir: OpenDirOpCB,
  pub readdir: ReadDirOpCB,
  pub releasedir: ReleaseDirOpCB,
  pub fsyncdir: FSyncDirOpCB,
  pub getxattr: GetXAttrOpCB,
  pub listxattr: ListXAttrOpCB,
  pub removexattr: RemoveXAttrOpCB,
  pub access: AccessOpCB,
}

#[napi(object)]
pub struct FileAttr {
  pub ino: i64,
  pub size: i64,
  pub mtime: i64,
  pub ctime: i64,
  pub btime: i64,
  pub kind: InodeKind,
  /// Permissions
  pub perm: u16,
  /// User id
  pub uid: u32,
  /// Group id
  pub gid: u32,
  /// Rdev
  pub rdev: u32,
  /// Flags (macOS only, see chflags(2))
  pub flags: u32,
}

#[napi]
pub enum FileAttrOrErr {
  Attr(FileAttr),
  Err(i32)
}

#[napi]
pub enum InodeKind {
  Directory,
  File,
  SymLink
}

pub fn to_file_type(kind: &InodeKind) -> FileType {
  match kind {
    InodeKind::Directory => FileType::Directory,
    InodeKind::File => FileType::RegularFile,
    InodeKind::SymLink => FileType::Symlink
  }
}

pub const BLOCK_SIZE: u64 = 4096;
fn blocks_in(size: u64) -> u64 {
  let d = size / BLOCK_SIZE;
  let r = size % BLOCK_SIZE;
  if r > 0 {
    d + 1
  } else {
    d
  }
}

impl FileAttr {
  pub fn into_fuse(&self) -> fuser::FileAttr {
    let mtime = system_time_from(self.mtime);
    fuser::FileAttr {
      atime: mtime,
      crtime: system_time_from(self.btime),
      ctime: system_time_from(self.ctime),
      flags: self.flags,
      gid: self.gid,
      uid: self.uid,
      ino: INodeNo(self.ino as u64),
      kind: to_file_type(&self.kind),
      mtime,
      nlink: 1,
      perm: self.perm,
      rdev: self.rdev,
      size: self.size as u64,
      blksize: BLOCK_SIZE as u32,
      blocks: blocks_in(self.size as u64),
    }
  }
}

fn system_time_from(millis: i64) -> SystemTime {
  SystemTime::UNIX_EPOCH + Duration::from_millis(millis as u64)
}

#[napi(object)]
pub struct AttrChanges {
  pub mode: Option<u32>,
  pub uid: Option<u32>,
  pub gid: Option<u32>,
  pub flags: Option<u32>,
}

#[napi(object)]
pub struct ParamsOfOpened {
  pub fh: i64,
  pub flags: u32
}

#[napi]
pub enum ParamsOfOpenedOrErr {
  Params(ParamsOfOpened),
  Err(i32)
}

#[napi]
pub enum BufferOrErr {
  Ok(Buffer),
  Err(i32)
}

#[napi(object)]
pub struct ReadArgs {
  pub offset: i64,
  pub size: u32,
  pub flags: i32,
  pub lock_owner: Option<i64>,
}

#[napi(object)]
pub struct ReleaseArgs {
  pub flags: i32,
  pub lock_owner: Option<i64>,
  pub flush: bool,
}

#[napi]
pub enum XAttrBytesOrErr {
  Data(Buffer),
  Size(u32),
  Err(i32)
}

#[napi(object)]
pub struct DirEntry {
  pub ino: i64,
  pub offset: i64,
  pub kind: InodeKind,
  pub name: String
}

#[napi]
pub enum DirListing {
  Lst(Vec<DirEntry>),
  Err(i32)
}

pub struct DirEntryPlus {
  pub offset: i64,
  pub kind: InodeKind,
  pub name: String,
}

#[napi(object)]
pub struct MkNodResult {
  pub ttl: i64,
  pub attr: FileAttr,
  pub generation: i64
}

#[napi]
pub enum NewEntryOrErr {
  Entry(MkNodResult),
  Err(i32)
}
