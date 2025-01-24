# db-reverie-engine
A 3D BSP-based game engine written in Rust for the DreamBox fantasy console

# Current Status
Broken 😔

Trying to port Quake 2 movement code - the bulk of the trace functions are in map.rs, and the trace movement function is in lib.rs
Beware, the code is horrible and not at all idiomatic Rust.

Current problem: player tends to "pop" towards walls while sliding along them, and can frequently get stuck in a solid and be unable to move
(for reference: what I am effectively trying to port from the Quake 2 source are CM_RecursiveHullCheck from cmodel.c, and PM_StepSlideMove_ from pmove.c)

# How to build
Outside of the usual `dbsdk-cli` build tool you'd use for a Dreambox game, it also expects some things to be present in the content folder:

- content/env should contain: sky1bk.ktx, sky1ft.ktx, sky1up.ktx, sky1dn.ktx, sky1lf.ktx, sky1rt.ktx (DXT1 format)
- content/maps should contain: demo1.bsp
- content/textures should contain: any texture referenced by demo1.bsp (with .ktx extension, DXT1 format) (technically not necessary to boot, you'll just get a ton of warnings and everything will show an error-texture placeholder instead)