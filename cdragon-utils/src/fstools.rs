
#[cfg(windows)]
pub fn symlink<P: AsRef<std::path::Path>, Q: AsRef<std::path::Path>>(src: P, dst: Q) -> std::io::Result<()> {
    if src.as_ref().is_dir() {
        std::os::windows::fs::symlink_dir(src, dst)
    } else {
        std::os::windows::fs::symlink_file(src, dst)
    }
}
#[cfg(unix)]
pub use std::os::unix::fs::symlink;

#[cfg(windows)]
pub use std::os::windows::fs::symlink_file;
#[cfg(unix)]
pub use std::os::unix::fs::symlink as symlink_file;

#[cfg(windows)]
pub use std::os::windows::fs::symlink_dir;
#[cfg(unix)]
pub use std::os::unix::fs::symlink as symlink_dir;

