import {test} from 'node:test';
import assert from 'node:assert/strict';
import {submittedRanges,attachShellColors} from '../src/lib/terminal-colors.ts';

test('submitted command tint excludes the green prompt and covers wrapped lines',()=>{
  assert.deepEqual(submittedRanges(10,18,12,80),[{line:10,x:18,width:62},{line:11,x:0,width:80},{line:12,x:0,width:80}]);
  assert.deepEqual(submittedRanges(-1,0,10,80),[]);
  assert.deepEqual(submittedRanges(5,8,4,80),[]);
});
test('shell markers color only command cells, leave alternate-screen apps untouched, and report literal Unicode paths',()=>{
  const handlers=new Map();const decorations=[];let directory='';let ready=false;
  const term={cols:80,options:{},buffer:{active:{type:'normal',baseY:0,cursorY:4,cursorX:15}},parser:{registerOscHandler(id,fn){handlers.set(id,fn);return{dispose(){handlers.delete(id);}};}},registerMarker(offset){let disposed=false;const callbacks=[];return{line:this.buffer.active.baseY+this.buffer.active.cursorY+offset,get isDisposed(){return disposed;},onDispose(fn){callbacks.push(fn);},dispose(){if(!disposed){disposed=true;callbacks.forEach(fn=>fn());}}};},registerDecoration(options){decorations.push(options);return{dispose(){}};}};
  const integration=attachShellColors(term,p=>directory=p,()=>ready=true);
  handlers.get(777)('svd-ready;bash');handlers.get(777)('svd-cwd;/home/résumé $literal');
  assert.equal(ready,true);assert.equal(directory,'/home/résumé $literal');assert.equal(term.options.theme.foreground,'#84beff');
  handlers.get(133)('B');term.buffer.active.cursorY=6;term.buffer.active.cursorX=0;handlers.get(133)('C');
  assert.equal(decorations.length,2);assert.equal(decorations[0].x,15);assert.equal(decorations[0].foregroundColor,'#ff7d87');
  term.buffer.active.type='alternate';assert.equal(handlers.get(133)('B'),false);
  integration.dispose();assert.equal(handlers.size,0);
});
