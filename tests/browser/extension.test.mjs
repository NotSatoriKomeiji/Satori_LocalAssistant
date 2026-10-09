import {test} from 'node:test';import assert from 'node:assert/strict';import {siteOrigin} from '../../browser-extension/privacy.js';
test('privacy normalization drops query, fragment and sensitive navigation',()=>{
 assert.equal(siteOrigin('https://www.bilibili.com/video/BV?token=secret#private'),'https://www.bilibili.com/');
 for(const value of ['http://example.com/','https://x.local/','file:///a','https://127.0.0.1/','https://user:pass@example.com/','https://example.com/login?secret=1','https://example.com/payment/','https://accounts.example.com/'])assert.equal(siteOrigin(value),null,value);
});
test('only focused non-incognito top-frame navigation produces bounded native metadata',async()=>{
 let events=[],tab={active:true,incognito:false,windowId:1},window={focused:true,incognito:false};
 globalThis.chrome={tabs:{get:async()=>tab},windows:{get:async()=>window},runtime:{sendNativeMessage:async(name,data)=>{events.push({name,data});return {accepted:true};}},storage:{local:{set:async()=>{}}},webNavigation:{onCommitted:{addListener:()=>{}}}};
 const {observe}=await import('../../browser-extension/background.js');const d={frameId:0,documentLifecycle:'active',tabId:1,url:'https://www.bilibili.com/video/private?secret=1'};
 await observe({...d,frameId:1});await observe({...d,documentLifecycle:'prerender'});tab.incognito=true;await observe(d);tab.incognito=false;window.focused=false;await observe(d);window.focused=true;
 assert.equal(events.length,0);await observe(d);await observe(d);assert.equal(events.length,1);
 assert.equal(events[0].name,'io.satori.browser');assert.deepEqual(Object.keys(events[0].data).sort(),['at','origin']);assert.equal(events[0].data.origin,'https://www.bilibili.com/');
});
