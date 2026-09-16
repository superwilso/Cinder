// Decompile the functions that reference given strings, plus their callers to a chosen depth,
// into ONE text file. Built for tracing the NW-ZX100's equalizer (analysis/RE_clear_bass.md):
// SpiderApp is stripped, but its DAL/GMI layers log their own function names, so a log string
// finds the function and the callers show where its arguments come from.
//
// Usage: -postScript TraceByStrings.java <out.txt> <callerDepth> <substring> [<substring> ...]
//@category Analysis

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.DataIterator;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public class TraceByStrings extends GhidraScript {

    private final Map<Function, String> seen = new LinkedHashMap<>();

    private List<Function> callersOf(Function f) {
        List<Function> out = new ArrayList<>();
        ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(f.getEntryPoint());
        while (refs.hasNext()) {
            Reference r = refs.next();
            if (!r.getReferenceType().isCall()) continue;
            Function c = getFunctionContaining(r.getFromAddress());
            if (c != null && !out.contains(c)) out.add(c);
        }
        return out;
    }

    private void add(Function f, String why, int depth) {
        if (f == null || seen.containsKey(f)) return;
        seen.put(f, why);
        if (depth <= 0) return;
        for (Function c : callersOf(f)) add(c, "caller of " + f.getName(), depth - 1);
    }

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length < 3) { println("usage: TraceByStrings <out.txt> <callerDepth> <substring>..."); return; }
        int depth = Integer.parseInt(args[1]);
        try (PrintWriter w = new PrintWriter(args[0], "UTF-8")) {
            for (int i = 2; i < args.length; i++) {
                String needle = args[i];
                w.println("### strings containing \"" + needle + "\"");
                DataIterator it = currentProgram.getListing().getDefinedData(true);
                while (it.hasNext() && !monitor.isCancelled()) {
                    Data d = it.next();
                    Object v = d.getValue();
                    if (!(v instanceof String) || !((String) v).contains(needle)) continue;
                    w.println("  " + d.getAddress() + "  " + v);
                    ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(d.getAddress());
                    while (refs.hasNext()) {
                        Reference r = refs.next();
                        Function f = getFunctionContaining(r.getFromAddress());
                        w.println("    ref " + r.getFromAddress() + (f == null ? " (no function)" : " in " + f.getName()));
                        add(f, "references \"" + v + "\"", depth);
                    }
                }
            }
            DecompInterface dec = new DecompInterface();
            dec.openProgram(currentProgram);
            for (Map.Entry<Function, String> e : seen.entrySet()) {
                Function f = e.getKey();
                w.println();
                w.println("======== " + f.getName() + " @ " + f.getEntryPoint() + "  (" + e.getValue() + ")");
                StringBuilder callees = new StringBuilder();
                for (Function c : f.getCalledFunctions(monitor)) callees.append(c.getName()).append(' ');
                w.println("// calls: " + callees);
                DecompileResults res = dec.decompileFunction(f, 180, monitor);
                w.println(res != null && res.decompileCompleted() ? res.getDecompiledFunction().getC()
                        : "// decompile failed: " + (res == null ? "null" : res.getErrorMessage()));
            }
            dec.dispose();
        }
        println("TraceByStrings: " + seen.size() + " functions -> " + args[0]);
    }
}
