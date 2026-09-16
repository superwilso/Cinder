// Decompile the functions at the given ABSOLUTE addresses (as Ghidra prints them) into one file.
// Usage: -postScript DecompileAddrs.java <out.txt> <addr> [<addr> ...]
//@category Analysis

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;

import java.io.PrintWriter;

public class DecompileAddrs extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        DecompInterface dec = new DecompInterface();
        dec.openProgram(currentProgram);
        try (PrintWriter w = new PrintWriter(args[0], "UTF-8")) {
            for (int i = 1; i < args.length; i++) {
                Function f = getFunctionContaining(toAddr(args[i]));
                if (f == null) { w.println("// no function at " + args[i]); continue; }
                w.println("======== " + f.getName() + " @ " + f.getEntryPoint());
                StringBuilder callees = new StringBuilder();
                for (Function c : f.getCalledFunctions(monitor)) callees.append(c.getName()).append(' ');
                w.println("// calls: " + callees);
                DecompileResults r = dec.decompileFunction(f, 180, monitor);
                w.println(r != null && r.decompileCompleted() ? r.getDecompiledFunction().getC()
                        : "// decompile failed");
            }
        }
        dec.dispose();
    }
}
