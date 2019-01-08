//! File access properties.

/*
Not supported due to complexity combined with low likelihood of ever being used:

- Low level direct VFD access: H5P{set,get}_driver, H5Pget_driver_info
- Direct file image API: H5P{set,get}_file_image, H5P{set,get}_file_image_callbacks
- Custom file access property lists in multi/family drivers
- Interfacing directly with multi/family parts via types/offsets
*/

use std::fmt::{self, Debug};
use std::iter;
use std::mem;
use std::ops::Deref;
use std::ptr;

use bitflags::bitflags;

use libhdf5_sys::h5f::{H5F_close_degree_t, H5F_mem_t, H5F_FAMILY_DEFAULT};
use libhdf5_sys::h5fd::H5FD_MEM_NTYPES;
use libhdf5_sys::h5fd::{
    H5FD_LOG_ALL, H5FD_LOG_FILE_IO, H5FD_LOG_FILE_READ, H5FD_LOG_FILE_WRITE, H5FD_LOG_FLAVOR,
    H5FD_LOG_FREE, H5FD_LOG_LOC_IO, H5FD_LOG_LOC_READ, H5FD_LOG_LOC_SEEK, H5FD_LOG_LOC_WRITE,
    H5FD_LOG_META_IO, H5FD_LOG_NUM_IO, H5FD_LOG_NUM_READ, H5FD_LOG_NUM_SEEK, H5FD_LOG_NUM_TRUNCATE,
    H5FD_LOG_NUM_WRITE, H5FD_LOG_TIME_CLOSE, H5FD_LOG_TIME_IO, H5FD_LOG_TIME_OPEN,
    H5FD_LOG_TIME_READ, H5FD_LOG_TIME_SEEK, H5FD_LOG_TIME_STAT, H5FD_LOG_TIME_TRUNCATE,
    H5FD_LOG_TIME_WRITE, H5FD_LOG_TRUNCATE,
};
use libhdf5_sys::h5p::{
    H5Pcreate, H5Pget_driver, H5Pget_fapl_core, H5Pget_fapl_family, H5Pget_fapl_multi,
    H5Pget_fclose_degree, H5Pset_fapl_core, H5Pset_fapl_family, H5Pset_fapl_log, H5Pset_fapl_multi,
    H5Pset_fapl_sec2, H5Pset_fapl_split, H5Pset_fapl_stdio, H5Pset_fclose_degree,
};

#[cfg(hdf5_1_8_13)]
use libhdf5_sys::h5p::{H5Pget_core_write_tracking, H5Pset_core_write_tracking};

use crate::globals::{
    H5FD_CORE, H5FD_FAMILY, H5FD_LOG, H5FD_MULTI, H5FD_SEC2, H5FD_STDIO, H5P_FILE_ACCESS,
};
use crate::internal_prelude::*;

/// File access properties.
#[repr(transparent)]
pub struct FileAccess(Handle);

impl ObjectClass for FileAccess {
    const NAME: &'static str = "file access property list";
    const VALID_TYPES: &'static [H5I_type_t] = &[H5I_GENPROP_LST];

    fn from_handle(handle: Handle) -> Self {
        FileAccess(handle)
    }

    fn handle(&self) -> &Handle {
        &self.0
    }

    fn validate(&self) -> Result<()> {
        let class = self.class()?;
        if class != PropertyListClass::FileAccess {
            fail!("expected file access property list, got {:?}", class);
        }
        Ok(())
    }
}

impl Debug for FileAccess {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let _e = silence_errors();
        let mut formatter = f.debug_struct("FileAccess");
        formatter.field("fclose_degree", &self.fclose_degree());
        formatter.field("driver", &self.driver());
        formatter.finish()
    }
}

impl Deref for FileAccess {
    type Target = PropertyList;

    fn deref(&self) -> &PropertyList {
        unsafe { self.transmute() }
    }
}

impl PartialEq for FileAccess {
    fn eq(&self, other: &Self) -> bool {
        <PropertyList as PartialEq>::eq(self, other)
    }
}

impl Eq for FileAccess {}

impl Clone for FileAccess {
    fn clone(&self) -> Self {
        unsafe { self.deref().clone().cast() }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CoreDriver {
    pub increment: usize,
    pub filebacked: bool,
    #[cfg(hdf5_1_8_13)]
    pub write_tracking: Option<usize>,
}

impl Default for CoreDriver {
    fn default() -> Self {
        Self {
            increment: 1024 * 1024,
            filebacked: false,
            #[cfg(hdf5_1_8_13)]
            write_tracking: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyDriver {
    pub member_size: usize,
}

impl Default for FamilyDriver {
    fn default() -> Self {
        Self { member_size: H5F_FAMILY_DEFAULT as _ }
    }
}

bitflags! {
    pub struct LogFlags: u64 {
        const TRUNCATE = H5FD_LOG_TRUNCATE;
        const META_IO = H5FD_LOG_META_IO;
        const LOC_READ = H5FD_LOG_LOC_READ;
        const LOC_WRITE = H5FD_LOG_LOC_WRITE;
        const LOC_SEEK = H5FD_LOG_LOC_SEEK;
        const LOC_IO = H5FD_LOG_LOC_IO;
        const FILE_READ = H5FD_LOG_FILE_READ;
        const FILE_WRITE = H5FD_LOG_FILE_WRITE;
        const FILE_IO = H5FD_LOG_FILE_IO;
        const FLAVOR = H5FD_LOG_FLAVOR;
        const NUM_READ = H5FD_LOG_NUM_READ;
        const NUM_WRITE = H5FD_LOG_NUM_WRITE;
        const NUM_SEEK = H5FD_LOG_NUM_SEEK;
        const NUM_TRUNCATE = H5FD_LOG_NUM_TRUNCATE;
        const NUM_IO = H5FD_LOG_NUM_IO;
        const TIME_OPEN = H5FD_LOG_TIME_OPEN;
        const TIME_STAT = H5FD_LOG_TIME_STAT;
        const TIME_READ = H5FD_LOG_TIME_READ;
        const TIME_WRITE = H5FD_LOG_TIME_WRITE;
        const TIME_SEEK = H5FD_LOG_TIME_SEEK;
        const TIME_TRUNCATE = H5FD_LOG_TIME_TRUNCATE;
        const TIME_CLOSE = H5FD_LOG_TIME_CLOSE;
        const TIME_IO = H5FD_LOG_TIME_IO;
        const FREE = H5FD_LOG_FREE;
        const ALL = H5FD_LOG_ALL;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogOptions {
    logfile: Option<String>,
    flags: LogFlags,
    buf_size: usize,
}

impl Default for LogOptions {
    fn default() -> Self {
        Self { logfile: None, flags: LogFlags::LOC_IO, buf_size: 0 }
    }
}

static FD_MEM_TYPES: &'static [H5F_mem_t] = &[
    H5F_mem_t::H5FD_MEM_DEFAULT,
    H5F_mem_t::H5FD_MEM_SUPER,
    H5F_mem_t::H5FD_MEM_BTREE,
    H5F_mem_t::H5FD_MEM_DRAW,
    H5F_mem_t::H5FD_MEM_GHEAP,
    H5F_mem_t::H5FD_MEM_LHEAP,
    H5F_mem_t::H5FD_MEM_OHDR,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiFile {
    pub name: String,
    pub addr: u64,
}

impl MultiFile {
    pub fn new(name: &str, addr: u64) -> Self {
        Self { name: name.into(), addr }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiLayout {
    pub mem_super: u8,
    pub mem_btree: u8,
    pub mem_draw: u8,
    pub mem_gheap: u8,
    pub mem_lheap: u8,
    pub mem_object: u8,
}

impl Default for MultiLayout {
    fn default() -> Self {
        Self { mem_super: 0, mem_btree: 1, mem_draw: 2, mem_gheap: 3, mem_lheap: 4, mem_object: 5 }
    }
}

impl MultiLayout {
    pub(crate) fn get(&self, index: usize) -> &u8 {
        match index {
            0 => &self.mem_super,
            1 => &self.mem_btree,
            2 => &self.mem_draw,
            3 => &self.mem_gheap,
            4 => &self.mem_lheap,
            5 => &self.mem_object,
            _ => unreachable!(),
        }
    }

    pub(crate) fn get_mut(&mut self, index: usize) -> &mut u8 {
        match index {
            0 => &mut self.mem_super,
            1 => &mut self.mem_btree,
            2 => &mut self.mem_draw,
            3 => &mut self.mem_gheap,
            4 => &mut self.mem_lheap,
            5 => &mut self.mem_object,
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiDriver {
    pub files: Vec<MultiFile>,
    pub layout: MultiLayout,
    pub relax: bool,
}

impl Default for MultiDriver {
    fn default() -> Self {
        let m = u64::max_value() / 6;
        let files = vec![
            MultiFile::new("%s-s.h5", 0 * m),
            MultiFile::new("%s-b.h5", 1 * m),
            MultiFile::new("%s-r.h5", 2 * m),
            MultiFile::new("%s-g.h5", 3 * m),
            MultiFile::new("%s-l.h5", 4 * m),
            MultiFile::new("%s-o.h5", 5 * m),
        ];
        Self { files, layout: MultiLayout::default(), relax: false }
    }
}

impl MultiDriver {
    pub(crate) fn validate(&self) -> Result<()> {
        let n = self.files.len();
        if self.files.is_empty() || n > 6 {
            fail!("invalid number of multi files: {} (expected 1-6)", n);
        }
        let mut used = iter::repeat(false).take(n).collect::<Vec<_>>();
        for i in 0..6 {
            let j = *self.layout.get(i) as usize;
            if j >= n {
                fail!("invalid multi layout index: {} (expected 0-{})", j, n);
            }
            used[j] = true;
        }
        if !used.iter().all(|x| *x) {
            fail!("invalid multi layout: some files are unused");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitDriver {
    meta_ext: String,
    raw_ext: String,
}

impl Default for SplitDriver {
    fn default() -> Self {
        Self { meta_ext: ".meta".into(), raw_ext: ".raw".into() }
    }
}

impl SplitDriver {
    pub(crate) fn from_multi(drv: &MultiDriver) -> Option<Self> {
        let layout = MultiLayout {
            mem_super: 0,
            mem_btree: 0,
            mem_draw: 1,
            mem_gheap: 1,
            mem_lheap: 0,
            mem_object: 0,
        };
        let is_split = drv.relax
            && drv.layout == layout
            && drv.files.len() == 2
            && drv.files[0].addr == 0
            && drv.files[1].addr == u64::max_value() / 2
            && drv.files[0].name.starts_with("%s")
            && drv.files[1].name.starts_with("%s");
        if !is_split {
            None
        } else {
            Some(SplitDriver {
                meta_ext: drv.files[0].name[2..].into(),
                raw_ext: drv.files[1].name[2..].into(),
            })
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDriver {
    Sec2,
    Stdio,
    Log,
    Core(CoreDriver),
    Family(FamilyDriver),
    Multi(MultiDriver),
    Split(SplitDriver),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileCloseDegree {
    Default,
    Weak,
    Semi,
    Strong,
}

impl Default for FileCloseDegree {
    fn default() -> Self {
        FileCloseDegree::Default
    }
}

impl From<H5F_close_degree_t> for FileCloseDegree {
    fn from(cd: H5F_close_degree_t) -> Self {
        match cd {
            H5F_close_degree_t::H5F_CLOSE_WEAK => FileCloseDegree::Weak,
            H5F_close_degree_t::H5F_CLOSE_SEMI => FileCloseDegree::Semi,
            H5F_close_degree_t::H5F_CLOSE_STRONG => FileCloseDegree::Strong,
            _ => FileCloseDegree::Default,
        }
    }
}

impl Into<H5F_close_degree_t> for FileCloseDegree {
    fn into(self) -> H5F_close_degree_t {
        match self {
            FileCloseDegree::Weak => H5F_close_degree_t::H5F_CLOSE_WEAK,
            FileCloseDegree::Semi => H5F_close_degree_t::H5F_CLOSE_SEMI,
            FileCloseDegree::Strong => H5F_close_degree_t::H5F_CLOSE_STRONG,
            _ => H5F_close_degree_t::H5F_CLOSE_DEFAULT,
        }
    }
}

/// Builder used to create file access property list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileAccessBuilder {
    file_driver: Option<FileDriver>,
    log_options: LogOptions,
    #[cfg(hdf5_1_8_13)]
    write_tracking: Option<usize>,
    fclose_degree: Option<FileCloseDegree>,
}

impl FileAccessBuilder {
    /// Creates a new file access property list builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new builder from an existing property list.
    pub fn from_plist(plist: &FileAccess) -> Result<Self> {
        let mut builder = Self::default();
        builder.fclose_degree(plist.get_fclose_degree()?);
        let drv = plist.get_driver()?;
        builder.driver(&drv);
        #[cfg(hdf5_1_8_13)]
        {
            if let FileDriver::Core(ref drv) = drv {
                builder.write_tracking(drv.write_tracking);
            }
        }
        Ok(builder)
    }

    pub fn fclose_degree(&mut self, value: FileCloseDegree) -> &mut Self {
        self.fclose_degree = Some(value);
        self
    }

    pub fn driver(&mut self, value: &FileDriver) -> &mut Self {
        self.file_driver = Some(value.clone());
        self
    }

    pub fn sec2(&mut self) -> &mut Self {
        self.driver(&FileDriver::Sec2)
    }

    pub fn stdio(&mut self) -> &mut Self {
        self.driver(&FileDriver::Stdio)
    }

    pub fn log_options(
        &mut self, logfile: Option<&str>, flags: LogFlags, buf_size: usize,
    ) -> &mut Self {
        self.log_options.logfile = logfile.map(Into::into);
        self.log_options.flags = flags;
        self.log_options.buf_size = buf_size;
        self.driver(&FileDriver::Log)
    }

    pub fn log(&mut self) -> &mut Self {
        self.log_options(None, LogFlags::LOC_IO, 0)
    }

    pub fn core_options(&mut self, increment: usize, filebacked: bool) -> &mut Self {
        let mut drv = CoreDriver::default();
        drv.increment = increment;
        drv.filebacked = filebacked;
        self.driver(&FileDriver::Core(drv))
    }

    pub fn core(&mut self) -> &mut Self {
        self.driver(&FileDriver::Core(CoreDriver::default()))
    }

    #[cfg(hdf5_1_8_13)]
    pub fn write_tracking(&mut self, page_size: Option<usize>) -> &mut Self {
        self.write_tracking = page_size;
        self
    }

    pub fn family(&mut self) -> &mut Self {
        self.driver(&FileDriver::Family(FamilyDriver::default()))
    }

    pub fn family_options(&mut self, member_size: usize) -> &mut Self {
        self.driver(&FileDriver::Family(FamilyDriver { member_size }))
    }

    pub fn multi_options(
        &mut self, files: &[MultiFile], layout: &MultiLayout, relax: bool,
    ) -> &mut Self {
        self.driver(&FileDriver::Multi(MultiDriver {
            files: files.to_vec(),
            layout: layout.clone(),
            relax,
        }))
    }

    pub fn multi(&mut self) -> &mut Self {
        self.driver(&FileDriver::Multi(MultiDriver::default()))
    }

    pub fn split_options(&mut self, meta_ext: &str, raw_ext: &str) -> &mut Self {
        self.driver(&FileDriver::Split(SplitDriver {
            meta_ext: meta_ext.into(),
            raw_ext: raw_ext.into(),
        }))
    }

    pub fn split(&mut self) -> &mut Self {
        self.driver(&FileDriver::Split(SplitDriver::default()))
    }

    fn set_log(&self, id: hid_t) -> Result<()> {
        let opt = &self.log_options;
        let flags = opt.flags.bits() as _;
        let buf_size = opt.buf_size as _;
        if let Some(ref logfile) = opt.logfile {
            let logfile = to_cstring(logfile.as_ref())?;
            h5try!(H5Pset_fapl_log(id, logfile.as_ptr(), flags, buf_size));
        } else {
            h5try!(H5Pset_fapl_log(id, ptr::null(), flags, buf_size));
        }
        Ok(())
    }

    fn set_core(&self, id: hid_t, drv: &CoreDriver) -> Result<()> {
        h5try!(H5Pset_fapl_core(id, drv.increment as _, drv.filebacked as _));
        #[cfg(hdf5_1_8_13)]
        {
            if let Some(page_size) = self.write_tracking {
                h5try!(H5Pset_core_write_tracking(id, 1, page_size.max(1) as _));
            } else {
                h5try!(H5Pset_core_write_tracking(id, 0, 1));
            }
        }
        Ok(())
    }

    fn set_family(id: hid_t, drv: &FamilyDriver) -> Result<()> {
        h5try!(H5Pset_fapl_family(id, drv.member_size as _, H5P_DEFAULT));
        Ok(())
    }

    fn set_multi(id: hid_t, drv: &MultiDriver) -> Result<()> {
        const N: usize = H5FD_MEM_NTYPES as _;
        debug_assert_eq!(FD_MEM_TYPES.len(), N as _);

        drv.validate()?;

        let mut memb_map: [H5F_mem_t; N] = unsafe { mem::zeroed() };
        let mut memb_fapl: [hid_t; N] = unsafe { mem::zeroed() };
        let mut memb_name: [*const c_char; N] = unsafe { mem::zeroed() };
        let mut memb_addr: [haddr_t; N] = unsafe { mem::zeroed() };

        let mut names = Vec::with_capacity(drv.files.len());
        for file in &drv.files {
            names.push(to_cstring(file.name.as_ref())?);
        }
        let default_name = to_cstring("%s-X.h5")?;

        for i in 0..N {
            memb_fapl[i] = H5P_DEFAULT;
            if i >= 1 {
                memb_map[i] = FD_MEM_TYPES[(1 + drv.layout.get(i - 1)) as usize];
            } else {
                memb_map[i] = H5F_mem_t::H5FD_MEM_DEFAULT;
            }
            if i == 0 {
                memb_name[i] = default_name.as_ptr();
                memb_addr[i] = 0;
            } else if i <= drv.files.len() {
                memb_name[i] = names[i - 1].as_ptr();
                memb_addr[i] = drv.files[i - 1].addr;
            } else {
                memb_name[i] = ptr::null();
                memb_addr[i] = 0;
            }
        }

        h5try!(H5Pset_fapl_multi(
            id,
            memb_map.as_ptr(),
            memb_fapl.as_ptr(),
            memb_name.as_ptr(),
            memb_addr.as_ptr(),
            drv.relax as _,
        ));

        Ok(())
    }

    fn set_split(id: hid_t, drv: &SplitDriver) -> Result<()> {
        let meta_ext = to_cstring(drv.meta_ext.as_ref())?;
        let raw_ext = to_cstring(drv.raw_ext.as_ref())?;
        h5try!(H5Pset_fapl_split(
            id,
            meta_ext.as_ptr(),
            H5P_DEFAULT,
            raw_ext.as_ptr(),
            H5P_DEFAULT
        ));
        Ok(())
    }

    fn set_driver(&self, id: hid_t, drv: &FileDriver) -> Result<()> {
        match drv {
            FileDriver::Sec2 => {
                h5try!(H5Pset_fapl_sec2(id));
            }
            FileDriver::Stdio => {
                h5try!(H5Pset_fapl_stdio(id));
            }
            FileDriver::Log => {
                self.set_log(id)?;
            }
            FileDriver::Core(drv) => {
                self.set_core(id, drv)?;
            }
            FileDriver::Family(drv) => {
                Self::set_family(id, drv)?;
            }
            FileDriver::Multi(drv) => {
                Self::set_multi(id, drv)?;
            }
            FileDriver::Split(drv) => {
                Self::set_split(id, drv)?;
            }
        }
        Ok(())
    }

    fn populate_plist(&self, id: hid_t) -> Result<()> {
        if let Some(ref v) = self.file_driver {
            self.set_driver(id, v)?;
        }
        if let Some(v) = self.fclose_degree {
            h5try!(H5Pset_fclose_degree(id, v.into()));
        }
        Ok(())
    }

    pub fn finish(&self) -> Result<FileAccess> {
        h5lock!({
            let plist = FileAccess::try_new()?;
            self.populate_plist(plist.id())?;
            Ok(plist)
        })
    }
}

/// File access property list.
impl FileAccess {
    pub fn try_new() -> Result<Self> {
        Self::from_id(h5try!(H5Pcreate(*H5P_FILE_ACCESS)))
    }

    pub fn build() -> FileAccessBuilder {
        FileAccessBuilder::new()
    }

    #[doc(hidden)]
    fn get_core(&self) -> Result<CoreDriver> {
        let mut drv = CoreDriver::default();
        let mut increment: size_t = 0;
        let mut filebacked: hbool_t = 0;
        h5try!(H5Pget_fapl_core(self.id(), &mut increment as *mut _, &mut filebacked as *mut _));
        drv.increment = increment as _;
        drv.filebacked = filebacked > 0;
        #[cfg(hdf5_1_8_13)]
        {
            let mut is_enabled: hbool_t = 0;
            let mut page_size: size_t = 0;
            h5try!(H5Pget_core_write_tracking(
                self.id(),
                &mut is_enabled as *mut _,
                &mut page_size as *mut _,
            ));
            if is_enabled > 0 {
                drv.write_tracking = Some(page_size as _);
            } else {
                drv.write_tracking = None;
            }
        }
        Ok(drv)
    }

    #[doc(hidden)]
    fn get_family(&self) -> Result<FamilyDriver> {
        let member_size = h5get!(H5Pget_fapl_family(self.id()): hsize_t, hid_t)?.0;
        Ok(FamilyDriver { member_size: member_size as _ })
    }

    #[doc(hidden)]
    fn get_multi(&self) -> Result<MultiDriver> {
        const N: usize = H5FD_MEM_NTYPES as _;
        debug_assert_eq!(FD_MEM_TYPES.len(), N as _);
        let mut memb_map: [H5F_mem_t; N] = unsafe { mem::zeroed() };
        let mut memb_fapl: [hid_t; N] = unsafe { mem::zeroed() };
        let mut memb_name: [*const c_char; N] = unsafe { mem::zeroed() };
        let mut memb_addr: [haddr_t; N] = unsafe { mem::zeroed() };
        let mut relax: hbool_t = 0;
        h5try!(H5Pget_fapl_multi(
            self.id(),
            memb_map.as_mut_ptr(),
            memb_fapl.as_mut_ptr(),
            memb_name.as_mut_ptr(),
            memb_addr.as_mut_ptr(),
            &mut relax as *mut _,
        ));
        let mut mapping: [u8; N] = unsafe { mem::zeroed() };
        let mut layout = MultiLayout::default();
        let mut files = Vec::new();
        for i in 1..N {
            let (map, name, addr) = (memb_map[i], memb_name[i], memb_addr[i]);
            let j = map as usize;
            ensure!(j < N, "member map index out of bounds: {} (expected 0-{})", j, N - 1);
            if mapping[j] == 0 {
                mapping[j] = 0xff - (files.len() as u8);
                files.push(MultiFile::new(&string_from_cstr(name), addr as _));
            }
            *layout.get_mut(i - 1) = 0xff - mapping[j];
        }
        let relax = relax > 0;
        let drv = MultiDriver { files, layout, relax };
        drv.validate().map(|_| drv)
    }

    #[doc(hidden)]
    pub fn get_driver(&self) -> Result<FileDriver> {
        let drv_id = h5try!(H5Pget_driver(self.id()));
        if drv_id == *H5FD_SEC2 {
            Ok(FileDriver::Sec2)
        } else if drv_id == *H5FD_STDIO {
            Ok(FileDriver::Stdio)
        } else if drv_id == *H5FD_LOG {
            Ok(FileDriver::Log)
        } else if drv_id == *H5FD_CORE {
            self.get_core().map(FileDriver::Core)
        } else if drv_id == *H5FD_FAMILY {
            self.get_family().map(FileDriver::Family)
        } else if drv_id == *H5FD_MULTI {
            let multi = self.get_multi()?;
            if let Some(split) = SplitDriver::from_multi(&multi) {
                Ok(FileDriver::Split(split))
            } else {
                Ok(FileDriver::Multi(multi))
            }
        } else {
            fail!("unknown file driver (id: {})", drv_id);
        }
    }

    pub fn driver(&self) -> Option<FileDriver> {
        self.get_driver().ok()
    }

    #[doc(hidden)]
    pub fn get_fclose_degree(&self) -> Result<FileCloseDegree> {
        h5get!(H5Pget_fclose_degree(self.id()): H5F_close_degree_t).map(|x| x.into())
    }

    pub fn fclose_degree(&self) -> FileCloseDegree {
        self.get_fclose_degree().unwrap_or_else(|_| FileCloseDegree::default())
    }
}
