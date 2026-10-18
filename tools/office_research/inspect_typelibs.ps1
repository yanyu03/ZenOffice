param([Parameter(Mandatory=$true)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$outPath = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $outPath) { throw 'Use a new output directory; existing evidence must not be overwritten.' }
New-Item -ItemType Directory -Path $outPath | Out-Null
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
public static class OfficeTypeEvidence {
  [DllImport("oleaut32.dll", CharSet=CharSet.Unicode, PreserveSig=true)]
  static extern int LoadTypeLibEx(string path, int regkind, out ITypeLib lib);
  public static object Read(string path, string[] selected) {
    ITypeLib lib; Marshal.ThrowExceptionForHR(LoadTypeLibEx(path, 2, out lib)); // REGKIND_NONE
    var types = new List<object>();
    try {
      IntPtr lp; lib.GetLibAttr(out lp);
      TYPELIBATTR la;
      try { la=Marshal.PtrToStructure<TYPELIBATTR>(lp); } finally { lib.ReleaseTLibAttr(lp); }
      for(int i=0;i<lib.GetTypeInfoCount();i++) {
        string name,doc,hf; int hc;
        lib.GetDocumentation(i,out name,out doc,out hc,out hf);
        TYPEKIND kind; lib.GetTypeInfoType(i,out kind);
        var members=new List<object>();
        if(Array.IndexOf(selected,name)>=0) {
          ITypeInfo ti; lib.GetTypeInfo(i,out ti);
          try {
            IntPtr ap; ti.GetTypeAttr(out ap); TYPEATTR a;
            try { a=Marshal.PtrToStructure<TYPEATTR>(ap); } finally { ti.ReleaseTypeAttr(ap); }
            for(int f=0;f<a.cFuncs;f++) {
              IntPtr fp; ti.GetFuncDesc(f,out fp);
              try {
                var fd=Marshal.PtrToStructure<FUNCDESC>(fp);
                var names=new string[Math.Max(1,fd.cParams+1)]; int count;
                ti.GetNames(fd.memid,names,names.Length,out count);
                var parameters=new List<object>();
                int size=Marshal.SizeOf<ELEMDESC>();
                for(int n=0;n<fd.cParams;n++) {
                  var ed=Marshal.PtrToStructure<ELEMDESC>(IntPtr.Add(fd.lprgelemdescParam,n*size));
                  parameters.Add(new {name=n+1<count?names[n+1]:null,vt=ed.tdesc.vt,flags=ed.desc.paramdesc.wParamFlags.ToString()});
                }
                members.Add(new {name=count>0?names[0]:null,dispid=fd.memid,invocation=fd.invkind.ToString(),return_vt=fd.elemdescFunc.tdesc.vt,parameters=parameters});
              } finally { ti.ReleaseFuncDesc(fp); }
            }
          } finally { Marshal.ReleaseComObject(ti); }
        }
        types.Add(new {name=name,kind=kind.ToString(),selected=Array.IndexOf(selected,name)>=0,members=members});
      }
      return new {path=path,registration="REGKIND_NONE",application_started=false,guid=la.guid.ToString(),major=la.wMajorVerNum,minor=la.wMinorVerNum,syskind=la.syskind.ToString(),type_count=types.Count,types=types};
    } finally { Marshal.ReleaseComObject(lib); }
  }
}
'@
$officeRoot='C:\Program Files\Microsoft Office\root\Office16'
$names=@('WINWORD.EXE','EXCEL.EXE','POWERPNT.EXE','MSWORD.OLB','MSPPT.OLB','WWLIB.DLL','PPCORE.DLL','XLCALL32.DLL')
$inventory=@(foreach($name in $names){
  $item=Get-Item -LiteralPath (Join-Path $officeRoot $name)
  $sig=Get-AuthenticodeSignature -LiteralPath $item.FullName
  [pscustomobject]@{name=$name;path=$item.FullName;bytes=$item.Length;file_version=$item.VersionInfo.FileVersion;sha256=(Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant();signature_status=$sig.Status.ToString();signer=$sig.SignerCertificate.Subject}
})
$inventory | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $outPath 'inventory.json') -Encoding utf8
$selected=@('_Application','Application','_Workbook','Workbook','Workbooks','_Worksheet','Worksheet','Worksheets','Range','_Document','Document','Documents','Paragraph','Paragraphs','Table','Tables','UndoRecord','_Presentation','Presentation','Presentations','Slides','_Slide','Slide','Shapes','Shape','TextFrame','TextRange')
foreach($target in @('MSWORD.OLB','EXCEL.EXE','MSPPT.OLB')){
  $data=[OfficeTypeEvidence]::Read((Join-Path $officeRoot $target),$selected)
  $data | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath (Join-Path $outPath ($target+'.typelib.json')) -Encoding utf8
  [pscustomobject]@{target=$target;type_count=$data.type_count;selected_types=@($data.types | Where-Object selected).Count;registered=$false;application_started=$false}|ConvertTo-Json -Compress
}
$after=@(foreach($item in $inventory){[pscustomobject]@{path=$item.path;unchanged=((Get-FileHash -LiteralPath $item.path -Algorithm SHA256).Hash.ToLowerInvariant() -eq $item.sha256)}})
$after | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outPath 'identity-after.json') -Encoding utf8
if(@($after | Where-Object {-not $_.unchanged}).Count) { throw 'Installed target changed during observation.' }
