use libc::{clock_gettime, clock_nanosleep};
use std::{io::Error, mem::MaybeUninit, ptr::null_mut, time::Duration};

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Timespec(libc::timespec);

impl Timespec {
    pub fn now() -> Self {
        let mut value = MaybeUninit::uninit();
        unsafe {
            let ret = clock_gettime(libc::CLOCK_MONOTONIC, value.as_mut_ptr());
            assert!(ret == 0);
            Self(value.assume_init())
        }
    }
    pub fn sleep_until(&self) {
        loop {
            let ret = unsafe {
                clock_nanosleep(
                    libc::CLOCK_MONOTONIC,
                    libc::TIMER_ABSTIME,
                    &self.0 as *const _,
                    null_mut(),
                )
            };
            let err = (ret != 0).then(Error::last_os_error);
            match err {
                Some(err) if err.raw_os_error() == Some(libc::EINTR) => continue,
                Some(err) => panic!("{err}"),
                _ => break,
            }
        }
    }
}

trait AddNanos {
    fn add_nanos(&mut self, nanos: u32) -> bool;
}

impl AddNanos for i32 {
    fn add_nanos(&mut self, nanos: u32) -> bool {
        *self += nanos as i32;
        if *self >= 1000000000 {
            *self -= 1000000000;
            true
        } else {
            false
        }
    }
}

impl AddNanos for i64 {
    fn add_nanos(&mut self, nanos: u32) -> bool {
        *self += Self::from(nanos as i32);
        if *self >= 1000000000 {
            *self -= 1000000000;
            true
        } else {
            false
        }
    }
}

impl core::ops::AddAssign<Duration> for Timespec {
    fn add_assign(&mut self, rhs: Duration) {
        self.0.tv_sec += rhs.as_secs() as i64 + self.0.tv_nsec.add_nanos(rhs.subsec_nanos()) as i64;
    }
}
