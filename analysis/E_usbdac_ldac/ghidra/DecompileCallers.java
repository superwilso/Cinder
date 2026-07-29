// Decompile every function that CALLS the function at the given address(es).
//
// Companion to DecompileStringXref.java. A log line usually lands you in a small helper (a getter,
// a validator); the logic you actually want is one frame up. This walks that edge without needing
// a symbol for either end.
//
// Addresses are file/vaddr as seen in objdump — the image base is added, since Ghidra rebases
// shared objects (the same correction DecompileAt.java needs).
//
// Usage: -postScript DecompileCallers.java 0x159b4
//@category Analysis

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

import java.util.LinkedHashSet;
import java.util.Set;

public class DecompileCallers extends GhidraScript {

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length < 1) {
            println("usage: DecompileCallers <addr> [<addr> ...]");
            return;
        }

        long base = currentProgram.getImageBase().getOffset();
        DecompInterface dec = new DecompInterface();
        dec.openProgram(currentProgram);

        for (String a : args) {
            String s = a.trim();
            if (s.startsWith("0x") || s.startsWith("0X")) {
                s = s.substring(2);
            }
            // Accept EITHER an objdump/file vaddr or an address as Ghidra prints it (already
            // rebased). Guessing wrong silently analyses a completely unrelated function, which is
            // how the first run of this script wasted a pass — so resolve it by what is actually
            // there: take the raw value if a function lives at it, otherwise add the image base.
            long raw = Long.parseLong(s, 16);
            Address target = currentProgram.getAddressFactory()
                    .getDefaultAddressSpace().getAddress(raw);
            Function callee = getFunctionContaining(target);
            if (callee == null) {
                Address rebased = currentProgram.getAddressFactory()
                        .getDefaultAddressSpace().getAddress(raw + base);
                Function f2 = getFunctionContaining(rebased);
                if (f2 != null) {
                    println("    (interpreting " + s + " as a file vaddr -> " + rebased + ")");
                    target = rebased;
                    callee = f2;
                }
            }
            println("=== callers of " + target
                    + (callee == null ? " (no function there)" : " (" + callee.getName() + ")"));

            Set<Function> callers = new LinkedHashSet<>();
            // References land on the entry point; if the address is mid-function, use the entry.
            Address refTo = callee != null ? callee.getEntryPoint() : target;
            ReferenceIterator refs =
                    currentProgram.getReferenceManager().getReferencesTo(refTo);
            while (refs.hasNext() && !monitor.isCancelled()) {
                Reference r = refs.next();
                Function f = getFunctionContaining(r.getFromAddress());
                if (f == null) {
                    println("    ref from " + r.getFromAddress() + " : no containing function");
                } else if (!f.equals(callee)) {
                    println("    called from " + r.getFromAddress() + " in " + f.getName());
                    callers.add(f);
                }
            }

            if (callers.isEmpty()) {
                println("    (no callers found — may be reached only through a vtable)");
                continue;
            }

            for (Function f : callers) {
                println("");
                println("======== " + f.getName() + " @ " + f.getEntryPoint() + " ========");
                DecompileResults res = dec.decompileFunction(f, 120, monitor);
                if (res != null && res.decompileCompleted()) {
                    println(res.getDecompiledFunction().getC());
                } else {
                    println("    decompile failed: "
                            + (res == null ? "null" : res.getErrorMessage()));
                }
            }
        }
        dec.dispose();
    }
}
