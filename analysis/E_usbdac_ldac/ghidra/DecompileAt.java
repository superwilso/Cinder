// DecompileAt — decompile functions by ADDRESS, not by symbol.
//
// The pst service-client stubs have no symbols of their own: each class exports only its factory,
// so every method sits unnamed inside one big `<Factory::CreateInstance@@Base>` blob. The vtable
// dump (analysis/tools/dump_vtable.py) gives their addresses and names; this turns those addresses
// into SIGNATURES, which is the part reading Thumb by hand gets wrong — and a wrong out-param
// guess writes through a bogus pointer.
//
//   analyzeHeadless <proj> <name> -import <lib.so> -postScript DecompileAt.java <addr>[,<addr>...]
//@category Cinder
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.lang.Register;
import ghidra.program.model.address.AddressSet;
import java.math.BigInteger;

public class DecompileAt extends GhidraScript {
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length < 1) { println("need a comma-separated address list"); return; }
        DecompInterface d = new DecompInterface();
        d.openProgram(currentProgram);
        for (String a : args[0].split(",")) {
            // Ghidra REBASES shared objects (0x100000 by default), so the vaddrs the vtable dump
            // prints are offsets from the image base, not absolute program addresses.
            long off = Long.parseLong(a.trim().replace("0x", ""), 16);
            Address addr = currentProgram.getImageBase().add(off);
            // These stubs are THUMB. Without TMode=1 the disassembler decodes ARM and produces
            // convincing nonsense, so set it before carving a function out.
            // Setting TMode is only needed where nothing is disassembled yet; where code already
            // exists Ghidra refuses the context write, and it is already Thumb anyway.
            try {
                Register tmode = currentProgram.getProgramContext().getRegister("TMode");
                if (tmode != null && getInstructionAt(addr) == null) {
                    currentProgram.getProgramContext()
                                  .setValue(tmode, addr, addr.add(4), BigInteger.ONE);
                }
            } catch (Exception ignored) { }
            // Every stub lives INSIDE one enormous `<Factory::CreateInstance@@Base>` function, so
            // the containing function is useless. Split ours out: remove the covering function,
            // then create one whose entry point is the stub itself.
            Function f = getFunctionContaining(addr);
            if (f != null && !f.getEntryPoint().equals(addr)) {
                removeFunctionAt(f.getEntryPoint());
                f = null;
            }
            if (f == null) {
                disassemble(addr);
                f = createFunction(addr, "stub_" + a.trim());
                if (f == null) f = getFunctionContaining(addr);
            }
            if (f == null) { println("=== " + a + " : could not create a function"); continue; }
            println("========== " + a + "  " + f.getName() + " ==========");
            DecompileResults r = d.decompileFunction(f, 90, monitor);
            println(r.decompileCompleted() ? r.getDecompiledFunction().getC()
                                           : "decompile failed: " + r.getErrorMessage());
        }
        d.dispose();
    }
}
