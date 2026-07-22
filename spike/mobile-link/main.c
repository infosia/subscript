/* Minimal C entry point for the P0.5 mobile link spike.
 * Calls the Cranelift-emitted exported function and exits 0.
 */

extern long long spike_main(void);

int main(void) {
    (void)spike_main();
    return 0;
}
