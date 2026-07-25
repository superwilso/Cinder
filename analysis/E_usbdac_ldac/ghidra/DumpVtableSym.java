// DumpVtableSym.java — dump a C++ vtable: resolve each pointer slot to its target
// function. Finds vtable labels by symbol-name regex (e.g. the mangled _ZTV name or
// Ghidra's "::vftable"), or takes a hex address. Prints slot index -> function.
// Usage: ... -postScript DumpVtableSym.java <outfile> <regex-or-0xADDR> [maxslots]
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.symbol.Symbol;
import ghidra.program.model.symbol.SymbolIterator;
import ghidra.program.model.listing.Function;
import java.io.FileWriter;
import java.io.PrintWriter;
import java.util.regex.Pattern;

public class DumpVtableSym extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        String out = (a.length > 0) ? a[0] : "/tmp/vtbl.txt";
        String sel = (a.length > 1) ? a[1] : "vftable";
        int max   = (a.length > 2) ? Integer.parseInt(a[2]) : 64;
        PrintWriter pw = new PrintWriter(new FileWriter(out));

        java.util.List<Address> bases = new java.util.ArrayList<>();
        if (sel.startsWith("0x") || sel.startsWith("0X")) {
            bases.add(toAddr(Long.parseLong(sel.substring(2), 16)));
        } else {
            Pattern pat = Pattern.compile(sel, Pattern.CASE_INSENSITIVE);
            SymbolIterator si = currentProgram.getSymbolTable().getAllSymbols(true);
            while (si.hasNext()) {
                Symbol s = si.next();
                if (pat.matcher(s.getName(true)).find())
                    pw.println("symbol: " + s.getName(true) + " @ " + s.getAddress());
                if (pat.matcher(s.getName()).find() || pat.matcher(s.getName(true)).find())
                    bases.add(s.getAddress());
            }
        }
        if (bases.isEmpty()) { pw.println("no vtable symbol matched: " + sel); pw.close(); return; }

        for (Address base : bases) {
            pw.println("\n==== vtable @ " + base + " ====");
            for (int i = 0; i < max; i++) {
                Address slot = base.add((long) i * 4);
                long p;
                try { p = getInt(slot) & 0xFFFFFFFFL; } catch (Exception e) { break; }
                if (p == 0) { pw.println("[" + i + "] 0x0"); continue; }
                Address tgt = toAddr(p);
                Function f = getFunctionAt(tgt);
                if (f == null) f = getFunctionContaining(tgt);
                String nm = (f != null) ? f.getName(true) : ("?@" + tgt);
                pw.println("[" + i + "] off " + (i*4) + "  -> " + nm);
            }
        }
        pw.close();
        println("DumpVtableSym -> " + out);
    }
}
