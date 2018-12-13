use std::fmt::{self, Debug};
use std::ops::Deref;
use std::ptr;

use libhdf5_sys::h5s::{
    H5Scopy, H5Screate_simple, H5Sget_simple_extent_dims, H5Sget_simple_extent_ndims,
};

use crate::internal_prelude::*;

/// Represents the HDF5 dataspace object.
#[repr(transparent)]
pub struct Dataspace(Handle);

impl ObjectClass for Dataspace {
    const NAME: &'static str = "dataspace";
    const VALID_TYPES: &'static [H5I_type_t] = &[H5I_DATASPACE];

    fn from_handle(handle: Handle) -> Self {
        Dataspace(handle)
    }

    fn handle(&self) -> &Handle {
        &self.0
    }

    fn short_repr(&self) -> Option<String> {
        if self.ndim() == 1 {
            Some(format!("({},)", self.dims()[0]))
        } else {
            let dims = self.dims().iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", ");
            Some(format!("({})", dims))
        }
    }
}

impl Debug for Dataspace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.debug_fmt(f)
    }
}

impl Deref for Dataspace {
    type Target = Object;

    fn deref(&self) -> &Object {
        unsafe { self.transmute() }
    }
}

impl Dataspace {
    pub fn try_new<D: Dimension>(d: D, resizable: bool) -> Result<Dataspace> {
        let rank = d.ndim();
        let mut dims: Vec<hsize_t> = vec![];
        let mut max_dims: Vec<hsize_t> = vec![];
        for dim in &d.dims() {
            dims.push(*dim as _);
            max_dims.push(if resizable { H5S_UNLIMITED } else { *dim as _ });
        }
        Dataspace::from_id(h5try!(H5Screate_simple(rank as _, dims.as_ptr(), max_dims.as_ptr())))
    }

    pub fn maxdims(&self) -> Vec<Ix> {
        let ndim = self.ndim();
        if ndim > 0 {
            let mut maxdims: Vec<hsize_t> = Vec::with_capacity(ndim);
            unsafe {
                maxdims.set_len(ndim);
            }
            if h5call!(H5Sget_simple_extent_dims(self.id(), ptr::null_mut(), maxdims.as_mut_ptr()))
                .is_ok()
            {
                return maxdims.iter().cloned().map(|x| x as _).collect();
            }
        }
        vec![]
    }

    pub fn resizable(&self) -> bool {
        self.maxdims().iter().any(|&x| x == H5S_UNLIMITED as _)
    }
}

impl Dimension for Dataspace {
    fn ndim(&self) -> usize {
        h5call!(H5Sget_simple_extent_ndims(self.id())).unwrap_or(0) as _
    }

    fn dims(&self) -> Vec<Ix> {
        let ndim = self.ndim();
        if ndim > 0 {
            let mut dims: Vec<hsize_t> = Vec::with_capacity(ndim);
            unsafe {
                dims.set_len(ndim);
            }
            if h5call!(H5Sget_simple_extent_dims(self.id(), dims.as_mut_ptr(), ptr::null_mut()))
                .is_ok()
            {
                return dims.iter().cloned().map(|x| x as _).collect();
            }
        }
        vec![]
    }
}

impl Clone for Dataspace {
    fn clone(&self) -> Dataspace {
        let id = h5call!(H5Scopy(self.id())).unwrap_or(H5I_INVALID_HID);
        Dataspace::from_id(id).ok().unwrap_or_else(Dataspace::invalid)
    }
}

#[cfg(test)]
pub mod tests {
    use crate::internal_prelude::*;

    #[test]
    pub fn test_dimension() {
        fn f<D: Dimension>(d: D) -> (usize, Vec<Ix>, Ix) {
            (d.ndim(), d.dims(), d.size())
        }

        assert_eq!(f(()), (0, vec![], 1));
        assert_eq!(f(&()), (0, vec![], 1));
        assert_eq!(f(2), (1, vec![2], 2));
        assert_eq!(f(&3), (1, vec![3], 3));
        assert_eq!(f((4,)), (1, vec![4], 4));
        assert_eq!(f(&(5,)), (1, vec![5], 5));
        assert_eq!(f((1, 2)), (2, vec![1, 2], 2));
        assert_eq!(f(&(3, 4)), (2, vec![3, 4], 12));
        assert_eq!(f(vec![2, 3]), (2, vec![2, 3], 6));
        assert_eq!(f(&vec![4, 5]), (2, vec![4, 5], 20));
    }

    #[test]
    pub fn test_debug() {
        assert_eq!(format!("{:?}", Dataspace::try_new((), true).unwrap()), "<HDF5 dataspace: ()>");
        assert_eq!(format!("{:?}", Dataspace::try_new(3, true).unwrap()), "<HDF5 dataspace: (3,)>");
        assert_eq!(
            format!("{:?}", Dataspace::try_new((1, 2), true).unwrap()),
            "<HDF5 dataspace: (1, 2)>"
        );
    }

    #[test]
    pub fn test_dataspace() {
        let _e = silence_errors();
        assert_err!(
            Dataspace::try_new(H5S_UNLIMITED as Ix, true),
            "current dimension must have a specific size"
        );

        let d = Dataspace::try_new((5, 6), true).unwrap();
        assert_eq!((d.ndim(), d.dims(), d.size()), (2, vec![5, 6], 30));

        assert_eq!(Dataspace::try_new((), true).unwrap().dims(), vec![]);

        assert_err!(Dataspace::from_id(H5I_INVALID_HID), "Invalid dataspace id");

        let dc = d.clone();
        assert!(dc.is_valid());
        assert_ne!(dc.id(), d.id());
        assert_eq!((d.ndim(), d.dims(), d.size()), (dc.ndim(), dc.dims(), dc.size()));

        assert_eq!(Dataspace::try_new((5, 6), false).unwrap().maxdims(), vec![5, 6]);
        assert_eq!(Dataspace::try_new((5, 6), false).unwrap().resizable(), false);
        assert_eq!(
            Dataspace::try_new((5, 6), true).unwrap().maxdims(),
            vec![H5S_UNLIMITED as _, H5S_UNLIMITED as _]
        );
        assert_eq!(Dataspace::try_new((5, 6), true).unwrap().resizable(), true);
    }
}
