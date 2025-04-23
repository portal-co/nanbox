#![no_std]

use core::{f64, marker::PhantomData, mem::take, ptr::NonNull};

use alloc::{boxed::Box, collections::btree_map::BTreeMap};
use spin::Mutex;
extern crate alloc;
static NANS: spin::Mutex<BTreeMap<u32, Entry>> = Mutex::new(BTreeMap::new());
enum Entry {
    F64(f64),
    Ptr(NonNull<()>),
}
unsafe impl Send for Entry {}
unsafe impl Sync for Entry {}

pub union NanBox<T> {
    raw: f64,
    raw_u64: u64,
    phantom: PhantomData<T>,
}
impl<T> NanBox<T> {
    pub fn new(a: f64) -> Self {
        if a.is_nan() {
            let mut l = NANS.lock();
            let mut i = 0;
            while l.contains_key(&i) {
                i += 1
            }
            l.insert(i, Entry::F64(a));
            let b = a.to_bits();
            unsafe {
                Self {
                    raw_u64: (b & !0xffffffff) | (i as u64),
                }
            }
        } else {
            unsafe { Self { raw: a } }
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        let i = unsafe {
            if !self.raw.is_nan() {
                return Some(self.raw);
            };
            let i = self.raw_u64 & 0xffffffff;
            i as u32
        };
        let mut l = NANS.lock();
        let Entry::F64(f) = l.get(&i)? else {
            return None;
        };
        Some(*f)
    }
    pub fn with_val(t: T) -> Self {
        let mut l = NANS.lock();
        let mut i = 0;
        while l.contains_key(&i) {
            i += 1
        }
        l.insert(
            i,
            Entry::Ptr(NonNull::new(Box::leak(Box::new(t)) as *mut T as *mut ()).unwrap()),
        );
        let b = f64::NAN.to_bits();
        unsafe {
            Self {
                raw_u64: (b & !0xffffffff) | (i as u64),
            }
        }
    }
    pub fn as_ref(&self) -> Option<&T> {
        let i = unsafe {
            if !self.raw.is_nan() {
                return None;
            };
            let i = self.raw_u64 & 0xffffffff;
            i as u32
        };
        let mut l = NANS.lock();
        let Entry::Ptr(p) = l.get(&i)? else {
            return None;
        };
        Some(unsafe { p.cast().as_ref() })
    }
    pub fn as_mut(&mut self) -> Option<&mut T> {
        let i = unsafe {
            if !self.raw.is_nan() {
                return None;
            };
            let i = self.raw_u64 & 0xffffffff;
            i as u32
        };
        let mut l = NANS.lock();
        let Entry::Ptr(p) = l.get(&i)? else {
            return None;
        };
        Some(unsafe { p.cast().as_mut() })
    }
    pub fn into_inner(mut self) -> Result<T, Self> {
        let r = take(unsafe { &mut self.raw_u64 });
        if !f64::from_bits(r).is_nan() {
            unsafe {
                self.raw_u64 = r;
            };
            return Err(self);
        };
        let i = (r & 0xffffffff) as u32;
        let mut l = NANS.lock();
        let p = match l.remove(&i) {
            Some(Entry::Ptr(p)) => p,
            mut p => {
                while let Some(p2) = p.take() {
                    p = l.insert(i, p2);
                }
                unsafe {
                    self.raw_u64 = r;
                };
                return Err(self);
            }
        };
        let a = unsafe { Box::from_raw(p.as_ptr() as *mut T) };
        Ok(*a)
    }
}
impl<T> Drop for NanBox<T> {
    fn drop(&mut self) {
        let i = unsafe {
            if !self.raw.is_nan() {
                return;
            };
            let i = self.raw_u64 & 0xffffffff;
            i as u32
        };
        let mut l = NANS.lock();
        let Some(Entry::Ptr(p)) = l.remove(&i) else {
            return;
        };
        let _ = unsafe { Box::from_raw(p.as_ptr() as *mut T) };
    }
}
