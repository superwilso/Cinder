// DecompileAll.java — Ghidra headless script: decompile every function in the
// current program to a single C-ish text dump for host-side grepping.
// Usage: analyzeHeadless <proj> <name> -process <prog> -noanalysis \
//          -scriptPath <thisdir> -postScript DecompileAll.java <outfile>
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionIterator;
import java.io.FileWriter;
import java.io.PrintWriter;

public class DecompileAll extends GhidraScript {
    @Override
    public void run() throws Exception {
        String out = (getScriptArgs().length > 0) ? getScriptArgs()[0] : "/tmp/decomp.c";
        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);
        PrintWriter pw = new PrintWriter(new FileWriter(out));
        FunctionIterator fns = currentProgram.getFunctionManager().getFunctions(true);
        int n = 0;
        while (fns.hasNext() && !monitor.isCancelled()) {
            Function f = fns.next();
            DecompileResults r = di.decompileFunction(f, 60, monitor);
            pw.println("// ======== " + f.getName() + " @ " + f.getEntryPoint() + " ========");
            if (r != null && r.decompileCompleted() && r.getDecompiledFunction() != null) {
                pw.println(r.getDecompiledFunction().getC());
            } else {
                pw.println("// (decompile failed)");
            }
            pw.println();
            n++;
        }
        pw.close();
        println("DecompileAll: wrote " + n + " functions to " + out);
    }
}
