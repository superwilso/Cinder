/* smfmf_probe — drive Sony's MMLib11.dll (Music Center's 12 Tone Analysis) without registering it.
 *
 * Research tool for analysis/RE_sensme_musiccenter.md. MMLib11.dll is Sony's and is NOT in this
 * repository: take it out of Music Center for PC's installer (§1 of that note) and put it next to
 * the .exe.
 *
 *   i686-w64-mingw32-gcc -O2 -o smfmf_probe.exe smfmf_probe.c -loleaut32 -lole32 -luuid -lm
 *   smfmf_probe.exe [audio.raw]     raw = s16le, 44.1 kHz, stereo; no argument = 90 s synthetic
 *
 * Prints the engine's parameters and every result ID; byte-array results are written to
 * result_<id>.bin in the current directory. Interface IDs and vtable slots are from the DLL's own
 * type library (the note's §2).
 */
#define COBJMACROS
#include <windows.h>
#include <oleauto.h>
#include <stdio.h>
#include <math.h>

static const GUID CLSID_MusicAnalysis2 = {0x8DE9ED2D,0x8CFB,0x4B33,{0xB8,0xE3,0xCE,0xE0,0x6F,0x29,0x03,0xE3}};
static const GUID IID_IMusicAnalysis2  = {0x0959C485,0xA191,0x4508,{0xA3,0x38,0x9E,0x2B,0x38,0x13,0xA1,0x66}};

typedef struct IAR2 IAR2;
typedef struct { /* IAnalysisResult2 */
    HRESULT (__stdcall *QI)(IAR2*, REFIID, void**); ULONG (__stdcall *AddRef)(IAR2*); ULONG (__stdcall *Release)(IAR2*);
    void* d3; void* d4; void* d5; void* d6;
    HRESULT (__stdcall *EnumResultIDs)(IAR2*, IEnumVARIANT**);
    HRESULT (__stdcall *GetResultByID)(IAR2*, INT, VARIANT*);
} IAR2Vtbl;
struct IAR2 { IAR2Vtbl* v; };

typedef struct IMA2 IMA2;
typedef struct { /* IMusicAnalysis2 */
    HRESULT (__stdcall *QI)(IMA2*, REFIID, void**); ULONG (__stdcall *AddRef)(IMA2*); ULONG (__stdcall *Release)(IMA2*);
    void* d3; void* d4; void* d5; void* d6;
    HRESULT (__stdcall *Run)(IMA2*);
    HRESULT (__stdcall *Stop)(IMA2*);
    HRESULT (__stdcall *GetPriorityRange)(IMA2*, SHORT*, SHORT*);
    HRESULT (__stdcall *SetPriority)(IMA2*, SHORT);
    HRESULT (__stdcall *GetPriority)(IMA2*, SHORT*);
    HRESULT (__stdcall *EnumParameterIDs)(IMA2*, IEnumVARIANT**);
    HRESULT (__stdcall *SetParameter)(IMA2*, INT, VARIANT);
    HRESULT (__stdcall *GetParameter)(IMA2*, INT, VARIANT*);
    HRESULT (__stdcall *InputPCM)(IMA2*, BYTE*, ULONG, LONG, LONG);
    HRESULT (__stdcall *SetInputPCMFormat)(IMA2*, USHORT, ULONG, USHORT);
    HRESULT (__stdcall *GetResult)(IMA2*, IAR2**);
    HRESULT (__stdcall *EnumResultIDs)(IMA2*, IEnumVARIANT**);
    HRESULT (__stdcall *IsSmfmfUpdatable)(IMA2*, BYTE*, ULONG);
} IMA2Vtbl;
struct IMA2 { IMA2Vtbl* v; };

static void show(const char* what, VARIANT* v, const char* dump_to)
{
    printf("  %s: vt=0x%x", what, v->vt);
    if (v->vt == VT_I4 || v->vt == VT_INT) printf(" = %ld", (long)v->lVal);
    else if (v->vt == VT_I2) printf(" = %d", v->iVal);
    else if (v->vt == VT_R8) printf(" = %f", v->dblVal);
    else if (v->vt == VT_R4) printf(" = %f", v->fltVal);
    else if (v->vt == VT_BOOL) printf(" = %d", v->boolVal);
    else if (v->vt == VT_BSTR) wprintf(L" = \"%ls\"", v->bstrVal);
    else if (v->vt & VT_ARRAY) {
        SAFEARRAY* sa = v->parray; LONG lo=0, hi=-1; SafeArrayGetLBound(sa,1,&lo); SafeArrayGetUBound(sa,1,&hi);
        ULONG n = (ULONG)(hi-lo+1); printf(" array elem=%lu n=%lu", (unsigned long)sa->cbElements, n);
        BYTE* p; if (SUCCEEDED(SafeArrayAccessData(sa,(void**)&p))) {
            ULONG bytes = n*sa->cbElements; printf(" [");
            for (ULONG i=0;i<bytes && i<64;i++) printf("%02x", p[i]);
            printf(bytes>64?"…]":"]");
            if (dump_to) { FILE* f=fopen(dump_to,"wb"); if(f){fwrite(p,1,bytes,f);fclose(f); printf(" -> %s", dump_to);} }
            SafeArrayUnaccessData(sa);
        }
    }
    printf("\n");
}

static void enum_ids(const char* label, IEnumVARIANT* e, IMA2* ma, IAR2* ar)
{
    VARIANT id; ULONG got; int k = 0;
    if (!e) { printf("%s: no enumerator\n", label); return; }
    while (IEnumVARIANT_Next(e, 1, &id, &got) == S_OK && got == 1) {
        VARIANT val; VariantInit(&val);
        VARIANT i4; VariantInit(&i4); VariantChangeType(&i4, &id, 0, VT_I4);
        char name[64]; snprintf(name, sizeof name, "%s id %ld", label, (long)i4.lVal);
        HRESULT hr = ar ? ar->v->GetResultByID(ar, i4.lVal, &val) : ma->v->GetParameter(ma, i4.lVal, &val);
        char dump[64]; snprintf(dump, sizeof dump, "result_%ld.bin", (long)i4.lVal);
        if (SUCCEEDED(hr)) show(name, &val, ar ? dump : NULL); else printf("  %s: hr=0x%lx\n", name, hr);
        VariantClear(&val); VariantClear(&id); k++;
    }
    printf("%s: %d ids\n", label, k);
    IEnumVARIANT_Release(e);
}

int main(int argc, char** argv)
{
    CoInitializeEx(NULL, COINIT_APARTMENTTHREADED);
    HMODULE h = LoadLibraryA("MMLib11.dll");
    if (!h) { printf("LoadLibrary failed %lu\n", GetLastError()); return 1; }
    HRESULT (__stdcall *gco)(REFCLSID, REFIID, void**) = (void*)GetProcAddress(h, "DllGetClassObject");
    IClassFactory* cf = NULL;
    HRESULT hr = gco(&CLSID_MusicAnalysis2, &IID_IClassFactory, (void**)&cf);
    printf("DllGetClassObject hr=0x%lx\n", hr); if (FAILED(hr)) return 1;
    IMA2* ma = NULL;
    hr = IClassFactory_CreateInstance(cf, NULL, &IID_IMusicAnalysis2, (void**)&ma);
    printf("CreateInstance hr=0x%lx\n", hr); if (FAILED(hr)) return 1;

    IEnumVARIANT* e = NULL;
    hr = ma->v->EnumParameterIDs(ma, &e); printf("EnumParameterIDs hr=0x%lx\n", hr);
    if (SUCCEEDED(hr)) enum_ids("param", e, ma, NULL);
    SHORT lo=0, hi=0; ma->v->GetPriorityRange(ma, &lo, &hi); printf("priority range %d..%d\n", lo, hi);

    /* PCM: a raw s16le 44.1 kHz stereo file if given, else 90 s of synthetic "music" */
    ULONG rate = 44100; ULONG frames; short* pcm;
    if (argc > 1) {
        FILE* f = fopen(argv[1], "rb"); if (!f) { printf("cannot open %s\n", argv[1]); return 1; }
        fseek(f, 0, SEEK_END); long sz = ftell(f); fseek(f, 0, SEEK_SET);
        frames = (ULONG)(sz / 4); pcm = malloc(frames * 4); fread(pcm, 4, frames, f); fclose(f);
    } else {
        frames = rate * 90; pcm = malloc(frames * 4);
        for (ULONG i = 0; i < frames; i++) {
            double t = (double)i / rate, beat = fmod(t, 0.5);
            double chord = sin(2*M_PI*220*t) + 0.6*sin(2*M_PI*277.18*t) + 0.5*sin(2*M_PI*329.63*t);
            double kick = beat < 0.08 ? sin(2*M_PI*60*t) * (1 - beat/0.08) * 2.0 : 0;
            double s = 0.18 * chord + 0.3 * kick;
            pcm[2*i] = (short)(s * 12000); pcm[2*i+1] = (short)(s * 11000);
        }
    }
    printf("pcm frames=%lu (%.1f s)\n", frames, (double)frames / rate);
    hr = ma->v->SetInputPCMFormat(ma, 2, rate, 16); printf("SetInputPCMFormat hr=0x%lx\n", hr);
    hr = ma->v->Run(ma); printf("Run hr=0x%lx\n", hr);
    const ULONG chunk = 4096 * 4;
    ULONG off = 0, calls = 0; DWORD t0 = GetTickCount();
    while (off < frames * 4) {
        ULONG n = frames * 4 - off < chunk ? frames * 4 - off : chunk;
        LONG last = off + n >= frames * 4;
        hr = ma->v->InputPCM(ma, (BYTE*)pcm + off, n, last, FALSE);
        if (FAILED(hr) || hr != S_OK) { printf("InputPCM at %lu hr=0x%lx\n", off, hr); if (FAILED(hr)) break; }
        off += n; calls++;
    }
    printf("fed %lu calls in %lu ms\n", calls, GetTickCount() - t0);
    IAR2* ar = NULL;
    hr = ma->v->GetResult(ma, &ar); printf("GetResult hr=0x%lx ar=%p\n", hr, (void*)ar);
    if (SUCCEEDED(hr) && ar) {
        e = NULL; hr = ar->v->EnumResultIDs(ar, &e); printf("EnumResultIDs hr=0x%lx\n", hr);
        if (SUCCEEDED(hr)) enum_ids("result", e, ma, ar);
    }
    e = NULL; hr = ma->v->EnumResultIDs(ma, &e); printf("MA EnumResultIDs hr=0x%lx\n", hr);
    return 0;
}
