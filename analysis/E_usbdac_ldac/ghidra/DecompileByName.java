// DecompileByName.java — Ghidra headless: decompile every function whose
// (demangled) name matches a case-insensitive regex. For mapping a C++ protocol
// where the symbols are present (e.g. appmgr/easel lifecycle).
// Usage: ... -postScript DecompileByName.java <outfile> <regex>
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionIterator;
import java.io.FileWriter;
import java.io.PrintWriter;
import java.util.regex.Pattern;

public class DecompileByName extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        String out = (a.length > 0) ? a[0] : "/tmp/byname.c";
        String rx  = (a.length > 1) ? a[1] : "Initialize";
        Pattern pat = Pattern.compile(rx, Pattern.CASE_INSENSITIVE);

        DecompInterface dec = new DecompInterface();
        dec.openProgram(currentProgram);
        PrintWriter pw = new PrintWriter(new FileWriter(out));
        int n = 0;
        FunctionIterator fi = currentProgram.getFunctionManager().getFunctions(true);
        while (fi.hasNext() && !monitor.isCancelled()) {
            Function f = fi.next();
            String nm = f.getName();              // demangled where Ghidra could
            if (!pat.matcher(nm).find()) continue;
            n++;
            pw.println("\n/* ===== " + nm + "  @ " + f.getEntryPoint() + " ===== */");
            try {
                DecompileResults r = dec.decompileFunction(f, 60, monitor);
                if (r != null && r.decompileCompleted())
                    pw.println(r.getDecompiledFunction().getC());
                else
                    pw.println("/* decompile failed: " + (r != null ? r.getErrorMessage() : "null") + " */");
            } catch (Exception e) {
                pw.println("/* exception: " + e.getMessage() + " */");
            }
        }
        pw.close();
        println("DecompileByName: matched " + n + " functions -> " + out);
    }
}
