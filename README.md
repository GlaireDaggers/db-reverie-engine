# db-reverie-engine
A 3D BSP-based game engine written in Rust for the DreamBox fantasy console

# Current Status
You can run around the map, jump, & crouch. Some brush entities are visible & have partial functionality, (func_door, func_rotating).
Static & skinned mesh entities are also supported.

# How to build
Outside of the usual `dbsdk-cli` build tool you'd use for a Dreambox game, it also expects some things to be present in the content folder:

- content/env should contain: sky1bk.ktx, sky1ft.ktx, sky1up.ktx, sky1dn.ktx, sky1lf.ktx, sky1rt.ktx (DXT1 format)
- content/maps should contain: demo1.bsp
- content/textures should contain: any texture referenced by demo1.bsp (with .ktx extension, DXT1 format) (technically not necessary to boot, you'll just get a ton of warnings and everything will show an error-texture placeholder instead)

(I'll happily send over the files I use for testing if you wanna DM me on Discord: `glairedaggers`)