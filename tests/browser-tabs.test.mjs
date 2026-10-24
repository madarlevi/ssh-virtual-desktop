import {test} from 'node:test';
import assert from 'node:assert/strict';
import {newTab,selectRelative,afterClose,addressUrl,shortcutAction} from '../src/lib/browser-tabs.ts';
test('tab switching and closing preserve independent page state',()=>{
 const a=newTab('a','https://example.com/a'),b=newTab('b','https://example.com/b');a.ready=true;a.title='Session A';b.ready=true;
 const tabs=[a,b];assert.equal(selectRelative(tabs.map(t=>t.id),'a',1),'b');assert.equal(selectRelative(['a','b'],'a',-1),'b');
 assert.equal(afterClose(['a','b'],'a','a'),'b');assert.equal(afterClose(['a','b'],'b','a'),'b');assert.equal(afterClose(['a'],'a','a'),'');
 assert.equal(a.url,'https://example.com/a');assert.equal(a.title,'Session A');assert.equal(b.url,'https://example.com/b');
});
test('browser shortcuts are scoped, including native-focused equivalents',()=>{
 for(const [key,action] of [['t','new'],['w','close'],['l','address'],['r','reload'],['j','downloads'],['f','find'],['9','tab-8']])assert.equal(shortcutAction(key,true,false,false),action);
 assert.equal(shortcutAction('Tab',true,true,false),'previous');assert.equal(shortcutAction('Tab',true,false,false),'next');assert.equal(shortcutAction('t',false,false,false),undefined);assert.equal(shortcutAction('ArrowLeft',false,false,true),'back');
});
test('address bar distinguishes search from HTTP navigation and refuses privileged schemes',()=>{
 assert.equal(addressUrl('example.com/a?q=hello'),'https://example.com/a?q=hello');assert.match(addressUrl('FreeBSD handbook'),/^https:\/\/duckduckgo.com\/\?q=FreeBSD%20handbook$/);
 assert.throws(()=>addressUrl('javascript:alert(1)'));assert.throws(()=>addressUrl('file:///C:/secret'));assert.throws(()=>addressUrl('https://user:secret@example.com/'));assert.equal(addressUrl(''),'');
});
