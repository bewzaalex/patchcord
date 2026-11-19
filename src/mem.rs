//! Keeping the process's memory down with glibc's allocator.
//!
//! Faces and other pictures are drawn on several threads at once (see `Scene::prepare`), and
//! glibc gives each thread an arena of its own: a big picture freed there stays with the process,
//! so every zoom or switch between the desk and the rack left tens of megabytes behind. Two arenas
//! are enough here, and after big pictures are let go the free memory goes back to the system.
//! (Elsewhere than glibc these do nothing.)

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod glibc {
    use std::ffi::c_int;

    unsafe extern "C" {
        fn mallopt(param: c_int, value: c_int) -> c_int;
        fn malloc_trim(pad: usize) -> c_int;
    }

    /// `M_ARENA_MAX` in glibc's malloc.h
    const M_ARENA_MAX: c_int = -8;

    pub fn limit_arenas() {
        // SAFETY: a plain setting of the allocator, made before any other thread starts
        unsafe { mallopt(M_ARENA_MAX, 2) };
    }

    pub fn give_back() {
        // SAFETY: returns free memory at the top of the heaps to the system; thread-safe
        unsafe { malloc_trim(0) };
    }
}

/// Few arenas for all the threads (call first thing, before any thread starts).
pub fn limit_arenas() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    glibc::limit_arenas();
}

/// Free memory back to the system: after big pictures were let go.
pub fn give_back() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    glibc::give_back();
}
