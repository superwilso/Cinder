// DecompileMatching.java — Ghidra headless: decompile only the functions that
// reference a defined string whose text matches a (case-insensitive) regex.
// Far faster than DecompileAll on large libraries.
// Usage: ... -postScript DecompileMatching.java <outfile> <regex>
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.DataIterator;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceManager;
import java.io.FileWriter;
import java.io.PrintWriter;
import java.util.LinkedHashSet;
import java.util.Set;
import java.util.regex.Pattern;

public class DecompileMatching extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        String out = (a.length > 0) ? a[0] : "/tmp/match.c";
        String rx = (a.length > 1) ? a[1] : "pcm";
        Pattern pat = Pattern.compile(rx, Pattern.CASE_INSENSITIVE);

        ReferenceManager rm = currentProgram.getReferenceManager();
        Set<Function> funcs = new LinkedHashSet<>();
        int hits = 0;
        DataIterator di = currentProgram.getListing().getDefinedData(true);
        while (di.hasNext() && !monitor.isCancelled()) {
            Data d = di.next();
            Object v = d.getValue();
            if (!(v instanceof String)) continue;
            if (!pat.matcher((String) v).find()) continue;
            hits++;
            for (Reference r : rm.getReferencesTo(d.getAddress())) {
                Function f = getFunctionContaining(r.getFromAddress());
                if (f != null) funcs.add(f);
            }
        }
        DecompInterface dec = new DecompInterface();
        dec.openProgram(currentProgram);
        PrintWriter pw = new PrintWriter(new FileWriter(out));
        pw.println("// regex=" + rx + "  matched-strings=" + hits + "  functions=" + funcs.size());
        for (Function f : funcs) {
            if (monitor.isCancelled()) break;
            DecompileResults r = dec.decompileFunction(f, 90, monitor);
            pw.println("// ======== " + f.getName() + " @ " + f.getEntryPoint() + " ========");
            if (r != null && r.decompileCompleted() && r.getDecompiledFunction() != null)
                pw.println(r.getDecompiledFunction().getC());
            else
                pw.println("// (decompile failed)");
            pw.println();
        }
        pw.close();
        println("DecompileMatching: " + funcs.size() + " functions (" + hits + " string hits) -> " + out);
    }
}
