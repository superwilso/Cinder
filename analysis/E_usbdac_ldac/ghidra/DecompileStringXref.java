// Decompile every function that references a given string literal.
//
// Why this exists: on these Sony libs the interesting code is usually findable by its LOG LINE, not
// by a symbol — the libs are stripped and -fno-rtti, but every service method logs
// "<File>.cc:<line>" through hagodaemon. Once a device run shows which log line was reached, this
// turns that line straight into source.
//
// It also sidesteps DecompileAt.java's weakness: no address guessing and no function creation, so
// it works on the ordinary service-side code where Ghidra's own analysis already got the bounds
// right. ARM PIC puts string addresses in PC-relative literal pools, so grep/objdump cannot follow
// these references — Ghidra's reference model can.
//
// Usage: -postScript DecompileStringXref.java "last device found"
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

import java.util.LinkedHashSet;
import java.util.Set;

public class DecompileStringXref extends GhidraScript {

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length < 1) {
            println("usage: DecompileStringXref <substring> [<substring> ...]");
            return;
        }

        DecompInterface dec = new DecompInterface();
        dec.openProgram(currentProgram);

        for (String needle : args) {
            println("=== searching for string: " + needle);
            Set<Function> hits = new LinkedHashSet<>();

            DataIterator it = currentProgram.getListing().getDefinedData(true);
            while (it.hasNext() && !monitor.isCancelled()) {
                Data d = it.next();
                Object v = d.getValue();
                if (!(v instanceof String)) {
                    continue;
                }
                if (!((String) v).contains(needle)) {
                    continue;
                }
                println("    string at " + d.getAddress() + " : " + v);

                ReferenceIterator refs =
                        currentProgram.getReferenceManager().getReferencesTo(d.getAddress());
                while (refs.hasNext()) {
                    Reference r = refs.next();
                    Address from = r.getFromAddress();
                    Function f = getFunctionContaining(from);
                    if (f == null) {
                        println("    ref from " + from + " : no containing function");
                    } else {
                        println("    ref from " + from + " in " + f.getName()
                                + " @ " + f.getEntryPoint());
                        hits.add(f);
                    }
                }
            }

            if (hits.isEmpty()) {
                println("    (no referencing function found)");
                continue;
            }

            for (Function f : hits) {
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
