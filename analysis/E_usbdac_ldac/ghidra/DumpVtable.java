// DumpVtable.java — find the BtTransmitterServiceClient vtable and print each
// slot's index + method name, so the LDAC bridge can call methods by index.
//
// Strategy: (1) map method-impl functions to names via the per-method log strings
// ("BtTransmitterServiceClient::<Method>"); (2) scan initialized memory for runs of
// code pointers (vtables); (3) for the run(s) that contain our client methods,
// print index -> name. The object's vptr points at slot 0 (Itanium/ARM ABI), so
// the printed index is exactly the value for vtable[index].
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.DataIterator;
import ghidra.program.model.listing.Function;
import ghidra.program.model.mem.MemoryBlock;
import ghidra.program.model.symbol.Reference;
import java.util.*;

public class DumpVtable extends GhidraScript {
    @Override
    public void run() throws Exception {
        var fm = currentProgram.getFunctionManager();
        var rm = currentProgram.getReferenceManager();
        var mem = currentProgram.getMemory();

        // 1. function-address -> method name, from defined strings + their xrefs.
        Map<Long, String> nameOf = new HashMap<>();
        DataIterator di = currentProgram.getListing().getDefinedData(true);
        while (di.hasNext()) {
            Data d = di.next();
            Object v = d.getValue();
            if (!(v instanceof String)) continue;
            String s = (String) v;
            int k = s.indexOf("BtTransmitterServiceClient::");
            if (k < 0) continue;
            String method = s.substring(k + "BtTransmitterServiceClient::".length());
            for (Reference r : rm.getReferencesTo(d.getAddress())) {
                Function f = getFunctionContaining(r.getFromAddress());
                if (f != null) nameOf.putIfAbsent(f.getEntryPoint().getOffset(), method);
            }
        }
        println("client methods mapped from strings: " + nameOf.size());

        // 2. scan initialized, non-executable blocks for runs of code pointers.
        List<long[]> runs = new ArrayList<>();   // [startAddr, count]
        List<List<Long>> runPtrs = new ArrayList<>();
        for (MemoryBlock b : mem.getBlocks()) {
            if (!b.isInitialized() || b.isExecute()) continue;
            Address a = b.getStart();
            long end = b.getEnd().getOffset();
            List<Long> cur = new ArrayList<>();
            long runStart = -1;
            for (long off = a.getOffset(); off + 4 <= end + 1; off += 4) {
                Address at = a.getNewAddress(off);
                long val;
                try { val = mem.getInt(at) & 0xffffffffL; } catch (Exception e) { break; }
                Address target = a.getNewAddress(val & ~1L); // thumb bit
                MemoryBlock tb = (val != 0) ? mem.getBlock(target) : null;
                boolean isCode = tb != null && tb.isExecute();
                if (isCode) {
                    if (runStart < 0) runStart = off;
                    cur.add(val & ~1L);
                } else {
                    if (cur.size() >= 4) { runs.add(new long[]{runStart, cur.size()}); runPtrs.add(cur); }
                    cur = new ArrayList<>(); runStart = -1;
                }
            }
            if (cur.size() >= 4) { runs.add(new long[]{runStart, cur.size()}); runPtrs.add(cur); }
        }

        // 3. print runs that contain >=2 of our client methods.
        for (int i = 0; i < runs.size(); i++) {
            List<Long> ptrs = runPtrs.get(i);
            int hits = 0;
            for (Long p : ptrs) if (nameOf.containsKey(p)) hits++;
            if (hits < 2) continue;
            long runStart = runs.get(i)[0];
            println(String.format("=== vtable candidate @ %08x  (%d slots, %d client methods) ===",
                    runStart, ptrs.size(), hits));
            // print the 6 words before the run to find the [offset-to-top=0, typeinfo] header
            Address base = currentProgram.getMinAddress();
            for (int w = 6; w >= 1; w--) {
                long off = runStart - 4L * w;
                Address at = base.getNewAddress(off);
                long val;
                try { val = mem.getInt(at) & 0xffffffffL; } catch (Exception e) { continue; }
                MemoryBlock vb = (val != 0) ? mem.getBlock(base.getNewAddress(val & ~1L)) : null;
                String cls = (val == 0) ? "ZERO(offset-to-top?)" : (vb != null && vb.isExecute()) ? "code" : "data(typeinfo?)";
                println(String.format("  pre[-%d] %08x = %08x  %s", w, off, val, cls));
            }
            for (int idx = 0; idx < ptrs.size(); idx++) {
                long p = ptrs.get(idx);
                String nm = nameOf.get(p);
                println(String.format("  [%3d] %08x %s", idx, p, nm != null ? nm : ""));
            }
        }
    }
}
