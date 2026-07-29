// FindAppBaseVtable.java — in a concrete easel app, locate the app class's
// easel::ApplicationBase-subobject vtable and recover the 2 pure virtuals (vfunc 0,1).
//
// Anchor: the app inherits (does not override) the tail methods StopBootAnimation
// (vfunc17), StartResumeAnimation (vfunc18), StopResumeAnimation (vfunc19), so 3
// consecutive vtable words point to those import thunks. From the StopBootAnimation
// slot, vfunc0 = slot-17*4, vfunc1 = slot-16*4 — the pure-virtual impls (local code).
// Dumps the vtable and decompiles vfunc0/vfunc1.
// Usage: ... -postScript FindAppBaseVtable.java <outfile>
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.mem.MemoryBlock;
import java.io.FileWriter;
import java.io.PrintWriter;

public class FindAppBaseVtable extends GhidraScript {
    String nameAt(long p) {
        if (p == 0) return null;
        Address t = toAddr(p);
        Function f = getFunctionAt(t);
        if (f == null) f = getFunctionContaining(t);
        return (f != null) ? f.getName(true) : null;
    }
    @Override public void run() throws Exception {
        String[] a = getScriptArgs();
        String out = (a.length > 0) ? a[0] : "/tmp/appbase.txt";
        PrintWriter pw = new PrintWriter(new FileWriter(out));
        DecompInterface dec = new DecompInterface(); dec.openProgram(currentProgram);

        for (MemoryBlock b : currentProgram.getMemory().getBlocks()) {
            if (!b.getName().contains("data.rel") && !b.getName().contains(".data")
                && !b.getName().contains("rodata")) continue;
            long start = b.getStart().getOffset(), end = b.getEnd().getOffset();
            for (long addr = start; addr + 12 <= end; addr += 4) {
                String n0, n1, n2;
                try {
                    n0 = nameAt(getInt(toAddr(addr)) & 0xFFFFFFFFL);
                    n1 = nameAt(getInt(toAddr(addr+4)) & 0xFFFFFFFFL);
                    n2 = nameAt(getInt(toAddr(addr+8)) & 0xFFFFFFFFL);
                } catch (Exception e) { continue; }
                if (n0 == null || n1 == null || n2 == null) continue;
                if (n0.contains("StopBootAnimation") && n1.contains("StartResumeAnimation")
                    && n2.contains("StopResumeAnimation")) {
                    long vf0 = addr - 17*4, vf1 = addr - 16*4;
                    pw.println("\n==== app ApplicationBase vtable: StopBootAnimation slot @ "
                        + toAddr(addr) + " ====");
                    for (int i = -2; i <= 20; i++) {  // include header + early slots
                        long sa = vf0 + (long)i*4;
                        if (sa < start) continue;
                        long w; try { w = getInt(toAddr(sa)) & 0xFFFFFFFFL; } catch (Exception e){ break; }
                        String nm = nameAt(w);
                        pw.println("vfunc " + i + " @ " + toAddr(sa) + " -> "
                            + (nm != null ? nm : String.format("0x%08x", w)));
                    }
                    for (long vf : new long[]{vf0, vf1}) {
                        long w = getInt(toAddr(vf)) & 0xFFFFFFFFL;
                        Function f = getFunctionAt(toAddr(w));
                        if (f == null) f = getFunctionContaining(toAddr(w));
                        pw.println("\n----- PURE VIRTUAL impl @ " + (f!=null?f.getEntryPoint():toAddr(w)) + " -----");
                        if (f != null) {
                            DecompileResults r = dec.decompileFunction(f, 60, monitor);
                            if (r != null && r.decompileCompleted())
                                pw.println(r.getDecompiledFunction().getC());
                            else pw.println("/* decompile failed */");
                        }
                    }
                    pw.flush();
                }
            }
        }
        pw.close();
        println("FindAppBaseVtable -> " + out);
    }
}
