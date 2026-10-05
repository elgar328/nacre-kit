# nacre-kit

**A convenience layer over the [nacre](https://github.com/elgar328/nacre) CAD kernel.** It turns code-CAD modeling steps into kernel operations, without making any geometric judgment of its own.

- **Reusable values.** The kernel consumes the solids it operates on. The kit copies a value before every operation that consumes it, so a script can use the same value as often as it likes.
- **Many solids at once.** A value can hold several bodies, and cut, fuse and common take any number of operands.
- **Sketch helpers.** Pen-style paths with arcs, chamfered corners and right-angle fillets.
- **Readable errors.** A kernel refusal comes back as a sentence, along with the step that caused it and, for a boolean with several operands, the pair that failed.
- **Files of what is shown.** `export_step` writes the bodies a build shows as a STEP file — not the copies and hidden values the model also holds — after raising any coordinate a long history left behind to the nearest `f64` of the exact geometry. `rendered_bodies` names the same bodies for other formats, such as the kernel's OBJ writer.

A front end records a script as a list of steps, and `build` replays them into a kernel model. The kit only composes kernel operations. Anything that needs a new exact decision, such as inside or outside, coincidence or direction, belongs in the kernel.

## Status

Experimental. The kit was built to exercise the kernel and to find out what a layer above it needs, so its API changes freely. [nacre-playground](https://github.com/elgar328/nacre-playground) is the browser app that drives it.

## Building

The kit depends on the kernel by path, so clone the two repositories side by side:

```sh
git clone https://github.com/elgar328/nacre
git clone https://github.com/elgar328/nacre-kit
cd nacre-kit
cargo test
```

## License

Licensed under either MIT or Apache-2.0, at your option.
