// Decompile functions by SYMBOL name, and every function that references a named DATA symbol.
// For modules with symbols (the ZX100's kernel drivers keep theirs).
// Usage: -postScript DecompileSymbols.java <out.txt> <name> [<name> ...]
//@category Analysis

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.Symbol;

import java.io.PrintWriter;
import java.util.LinkedHashSet;
import java.util.Set;

public class DecompileSymbols extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        Set<Function> todo = new LinkedHashSet<>();
        try (PrintWriter w = new PrintWriter(args[0], "UTF-8")) {
            for (int i = 1; i < args.length; i++) {
                for (Symbol s : currentProgram.getSymbolTable().getSymbols(args[i])) {
                    Function f = getFunctionAt(s.getAddress());
                    if (f != null) { todo.add(f); continue; }
                    w.println("// data symbol " + args[i] + " @ " + s.getAddress() + " referenced from:");
                    for (Reference r : getReferencesTo(s.getAddress())) {
                        Function c = getFunctionContaining(r.getFromAddress());
                        w.println("//   " + r.getFromAddress() + (c == null ? "" : " in " + c.getName()));
                        if (c != null) todo.add(c);
                    }
                }
            }
            DecompInterface dec = new DecompInterface();
            dec.openProgram(currentProgram);
            for (Function f : todo) {
                w.println();
                w.println("======== " + f.getName() + " @ " + f.getEntryPoint());
                DecompileResults r = dec.decompileFunction(f, 180, monitor);
                w.println(r != null && r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// decompile failed");
            }
            dec.dispose();
        }
    }
}
