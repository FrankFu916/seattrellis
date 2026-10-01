# Interpretation and integration limits

The production Rust solver and OR-Tools CP-SAT are different algorithms with
different preparation work. The experiment's CP-SAT calls a native C++ engine
through a Python adapter; it is not a Python-only solver. Conversely, timing
the two paths does not establish that Rust or C++ is intrinsically faster.
A language comparison would require the same algorithm, data structures,
optimization level, workload, deadline, and correctness checks implemented in
both languages.

The installed OR-Tools 9.14.6206 wheel is Apache 2.0 according to its package
metadata. Its Linux x86_64 distribution includes the native CP-SAT wrapper and
`libortools.so.9`, as well as protobuf, Abseil, and additional solver libraries.
The wheel's total installed size is about 73.3 MiB, including 69.5 MiB of native
libraries. `libortools.so.9` alone is about 29.2 MiB. This is the whole wheel,
not the minimum footprint of a tailored C++ CP-SAT deployment; it includes
components this experiment does not use. It also excludes the Python runtime
and separately installed Python dependencies. `packaging-footprint.json`
records exact byte counts and scope.

Keeping the existing Rust domain model and application boundary permits native
SwiftUI/AppKit, WinUI 3, Android Compose/Material, Qt 6, or GTK frontends to share
one set of rules, persistence, and export behavior. A C++ Qt frontend does not
require rewriting those modules in C++. Native adapters can call a coarse
application API over an ABI, generated binding, or local process protocol;
the UI toolkit should not become the owner of seating rules or history.

A CP-SAT backend could live behind the same solver contract and return the
existing feasible/infeasible/unknown outcomes. Before it becomes a production
alternative, its adapter must cover all supported inputs and objectives, keep
the shared deadline and cancellation contract, and validate every assignment
with the existing domain validator. Floating teacher objectives, fairness
history, repeat-neighbor penalties, candidate diversity, repair locks, and
quality scoring are not modeled in the present hard-feasibility experiment.
Integer scaling or approximation of costs must be explicit and tested.

A standalone SAT solver would need a separate encoding and benchmark.
Assignment, uniqueness, and adjacency can be expressed with Boolean clauses;
graph/distance tables and teacher objectives add encoding and optimization
work. CP-SAT already combines SAT-based reasoning with constraint processing,
so its result cannot be presented as a measured result for every SAT solver.
MaxSAT or pseudo-Boolean optimization would need its own objective translation.

For C++ integration, verify native dependencies, compiler/standard-library ABI,
license notices, packaging, and architectures on each actual target. This
experiment executes the Linux x86_64 wheel only. It does not compile a C++
adapter on Windows or macOS, build iOS/Android SDKs, test universal macOS
binaries, or verify WebAssembly support. The Python adapter is disposable
benchmark infrastructure, not a recommendation to distribute Python with
the desktop or mobile products.

The engineering recommendation is to retain the current Rust solver as the
default and use these results as a bounded feasibility baseline. A potential
CP-SAT backend should be evaluated further on difficult real classroom
instances with equivalent objective semantics and target-specific packaging.
The present corpus and measurement scope cannot justify a wholesale rewrite
or prove one backend universally superior.
