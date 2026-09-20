// List every reference to the given ABSOLUTE addresses and decompile each referencing function.
// Usage: -postScript XrefsDecompile.java <out.txt> <addr> [<addr> ...]
//@category Analysis

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

import java.io.PrintWriter;
import java.util.LinkedHashSet;
import java.util.Set;

public class XrefsDecompile extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        DecompInterface dec = new DecompInterface();
        dec.openProgram(currentProgram);
        Set<Function> seen = new LinkedHashSet<>();
        try (PrintWriter w = new PrintWriter(args[0], "UTF-8")) {
            for (int i = 1; i < args.length; i++) {
                Address a = toAddr(args[i]);
                w.println("======== xrefs to " + a);
                ReferenceIterator it = currentProgram.getReferenceManager().getReferencesTo(a);
                int n = 0;
                while (it.hasNext()) {
                    Reference r = it.next();
                    Function f = getFunctionContaining(r.getFromAddress());
                    w.println("  from " + r.getFromAddress() + "  " + r.getReferenceType()
                              + "  in " + (f == null ? "<none>" : f.getName() + " @ " + f.getEntryPoint()));
                    if (f != null) seen.add(f);
                    n++;
                }
                w.println("  (" + n + " references)");
            }
            for (Function f : seen) {
                w.println();
                w.println("======== " + f.getName() + " @ " + f.getEntryPoint());
                DecompileResults res = dec.decompileFunction(f, 120, monitor);
                w.println(res.decompileCompleted() ? res.getDecompiledFunction().getC()
                                                   : "// decompile failed: " + res.getErrorMessage());
            }
        }
    }
}
