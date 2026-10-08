//! Wider native ABI coverage. The contract is compiler.md §179.2, acceptance 2.
#[cfg(not(all(windows, target_env = "msvc")))]
#[path = "abi_wide/sweep.rs"]
mod sweep;
fn main() {
    #[cfg(not(all(windows, target_env = "msvc")))]
    sweep::run();
    #[cfg(all(windows, target_env = "msvc"))]
    println!("native fixture requires a C compiler with _Float16");
}
