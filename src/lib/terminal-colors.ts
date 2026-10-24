import type {Terminal,IMarker,IDisposable} from '@xterm/xterm';
export const terminalPalette={background:'#0c1520',foreground:'#84beff',cursor:'#f2f6fc',selectionBackground:'#31537088',black:'#142333',red:'#ff7d87',green:'#71dda2',yellow:'#e7c984',blue:'#84beff',magenta:'#bd9af7',cyan:'#71d7df',white:'#f2f6fc',brightBlack:'#72869c',brightRed:'#ff9da5',brightGreen:'#a0efbf',brightYellow:'#f4dfaa',brightBlue:'#a8d2ff',brightMagenta:'#d8bfff',brightCyan:'#9be9ef',brightWhite:'#ffffff'};
export function submittedRanges(startLine:number,startColumn:number,endLine:number,columns:number){
  if(startLine<0||endLine<startLine||endLine-startLine>500)return[];
  return Array.from({length:endLine-startLine+1},(_,i)=>({line:startLine+i,x:i===0?Math.max(0,Math.min(columns-1,startColumn)):0,width:columns-(i===0?Math.max(0,Math.min(columns-1,startColumn)):0)}));
}
export function attachShellColors(term:Terminal,onDirectory:(path:string)=>void,onReady:()=>void):IDisposable {
  let input:IMarker|undefined;let column=0;
  const marks=new Set<IMarker>();
  const hooks=[term.parser.registerOscHandler(777,data=>{
    if(data==='svd-ready;bash'){term.options.theme={...terminalPalette};onReady();return true;}
    if(data.startsWith('svd-cwd;')){onDirectory(data.slice(8).replace(/[\u0000-\u001f\u007f]/g,'').slice(0,4096));return true;}return false;
  }),term.parser.registerOscHandler(133,data=>{
    if(term.buffer.active.type!=='normal')return false;
    const code=data.split(';')[0];
    if(code==='A'){input?.dispose();input=undefined;}
    if(code==='B'){input?.dispose();input=term.registerMarker(0);column=term.buffer.active.cursorX;}
    if(code==='C'&&input&&!input.isDisposed){
      const buffer=term.buffer.active;const current=buffer.baseY+buffer.cursorY;
      const end=current-(buffer.cursorX===0?1:0);
      for(const range of submittedRanges(input.line,column,end,term.cols)){
        const marker=term.registerMarker(range.line-current);if(!marker)continue;
        const decoration=term.registerDecoration({marker,x:range.x,width:range.width,foregroundColor:'#ff7d87',layer:'top'});
        if(!decoration){marker.dispose();continue;}
        marks.add(marker);marker.onDispose(()=>{decoration.dispose();marks.delete(marker);});
      }
      input.dispose();input=undefined;
    }
    return ['A','B','C','D'].includes(code);
  })];
  return{dispose(){hooks.forEach(h=>h.dispose());input?.dispose();for(const marker of marks)marker.dispose();marks.clear();}};
}
