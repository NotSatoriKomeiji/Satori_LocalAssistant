// Interface preview only; the real event scheduler and learner live in Rust.
import type { Status, Mode, Settings } from './types';
let seq=0;
const state:Status={
  experience:{rules:[],pending:[],question:null,local_hits:0,automatic_calls_today:0},demo:true,settings:{paused:false,automatic_learning:true,auto_adjustments:true,time_recommendations:true,floating_cards:true,associations:true,website_observation:false,app_min_days:2,app_threshold:.50,app_margin:.08,suggest_threshold:.50,auto_threshold:.50,min_auto_samples:3,min_auto_sessions:3,max_volume:.8,max_auto_delta:.05,retention_days:90},
  current:{app_id:'preview-game',app_name:'Game.exe（模拟）',device:'preview-speakers',hour:new Date().getHours(),weekday:(new Date().getDay()+6)%7,local_day:new Date().getFullYear()*10000+(new Date().getMonth()+1)*100+new Date().getDate(),session:'preview',recent:[],activity:'application',power:null},
  memories:[],targets:[],app_recommendation:null,quick_apps:[],
  capabilities:[{id:'open_website',name:'打开站点首页',available:true,autonomous:false,authorization:'仅明确点击，通过默认浏览器打开HTTPS首页'},{id:'volume',name:'主音量',available:true,autonomous:true,authorization:'默认逐次确认；自动调整需应用单独授权'},{id:'brightness',name:'屏幕亮度',available:false,autonomous:true,authorization:'未实现，不能请求或执行亮度动作'},{id:'open_app',name:'打开已选应用',available:true,autonomous:false,authorization:'仅明确点击；不能自动启动'}],
  extensions:[{id:'associations',name:'应用联想',api_version:1,scope:'usage_metadata'}],
  startup:{supported:true,enabled:false,simulated:true,message:'演示状态，不修改系统启动项'},
  browser_last_event:0,
  volume:.5,apps:[{id:'preview-game',name:'Game.exe（模拟）',observe:true,volume:'ask',sensitive:false,explicit:false}],
  proposal:null,journal:[],samples:0,
  events:[{app_name:'Game.exe（模拟）',kind:'application_focus',at:Date.now()/1000}],event_count:1,
  message:'普通场景自动记录 · 网页仅演示界面，未连接系统',
};
const effective=()=>{const p=state.apps.find(p=>p.id===state.current?.app_id);return p?.observe && (!p.sensitive||p.explicit) && (state.settings.automatic_learning||p.explicit) && !state.settings.paused;};
export async function preview(name:string,args:Record<string,unknown>):Promise<Status>{
  const c=state.current;
  if(name==='add_website'||name==='demo_websites'){
    const value=name==='demo_websites'?'https://www.bilibili.com/':String(args.url);
    const parsed=new URL(value);if(parsed.protocol!=='https:'||parsed.username||parsed.password||parsed.port)throw '网址格式无效';
    const origin=parsed.origin+'/';const id='web_'+parsed.hostname;
    const old=state.apps.find(p=>p.id===id);if(old&&(!old.observe||old.sensitive))throw '该网站已停用';
    if(!old)state.apps.push({id,name:parsed.hostname,observe:true,volume:'off',sensitive:false,explicit:true});
    if(!state.targets.some(t=>t.app_id===id))state.targets.push({kind:'website',app_id:id,name:parsed.hostname,enabled:true});
    if(name==='demo_websites') {state.settings.website_observation=true;state.memories.push({app_id:id,name:parsed.hostname,pinned:false,importance:.25,last_used:Date.now()/1000,days:3,samples:0});}
    state.message='模拟站点已添加，不读取真实浏览器';
  }else if(name==='refresh'  && c)c.hour=new Date().getHours();
  if(name==='set_startup'){state.startup.enabled=Boolean(args.enabled);}
  else if(name==='demo_grid'&&c){
    if(!effective()||state.apps.find(p=>p.id===c.app_id)?.sensitive)throw '请恢复普通场景的学习';
    for(const name of ['Editor','Browser','Notes','Music','Terminal','Video']){
      const id='preview-'+name.toLowerCase();
      if(!state.apps.some(p=>p.id===id))state.apps.push({id,name:name+'.exe（模拟）',observe:true,volume:'ask',sensitive:false,explicit:false});
      const permission=state.apps.find(p=>p.id===id);if(!permission?.observe || permission.sensitive)continue;
      if(!state.targets.some(t=>t.app_id===id))state.targets.push({kind:'app',app_id:id,name:permission.name,enabled:true});
      if(!state.memories.some(m=>m.app_id===id))state.memories.push({app_id:id,name:permission.name,pinned:false,importance:.35,last_used:Date.now()/1000-86400,days:8,samples:0});
    }state.message='已加载6个模拟应用';
  }else if(name==='open_target'){
    if(args.session!==c?.session || !state.quick_apps.some(a=>a.app_id===args.id))throw '推荐目标已失效';
    state.app_recommendation=null;state.message=String(args.id).startsWith('web_')?'模拟打开网站，未调用真实浏览器':'模拟打开应用，未启动真实程序';
  }else if(name==='open_panel'||name==='hide_popup'){}
  else if(name==='grant_current'&&c){
    const p=state.apps.find(p=>p.id===c.app_id);
    if(p){p.observe=true;p.volume='ask';p.explicit=true;}
  }else if(name==='set_permission'){
    const p=state.apps.find(p=>p.id===args.id);
    if(p){p.observe=Boolean(args.observe);p.volume=args.mode as Mode;p.explicit=true;state.proposal=null;state.app_recommendation=null;}
  }else if(name==='set_sensitive'){
    const p=state.apps.find(p=>p.id===args.id);
    if(p){p.sensitive=Boolean(args.sensitive);if(p.sensitive){p.observe=false;p.volume='off';p.explicit=false;}state.proposal=null;state.app_recommendation=null;}
  }else if(name==='save_settings'){
    const next=args.settings as Settings;
    if(next.automatic_learning && !state.settings.automatic_learning){
      for(const p of state.apps){if(!p.explicit&&!p.sensitive){p.observe=true;p.volume='ask';}}
    }
    state.settings=structuredClone(next);state.proposal=null;state.app_recommendation=null;
  }else if(name==='demo_scene'&&c){
    const scene=String(args.scene);const names:Record<string,string>={game:'Game.exe（模拟）',editor:'Editor.exe（模拟）',browser:'Browser.exe（模拟）',desktop:'桌面（模拟）',sensitive:'KeePass.exe'};
    c.app_id='preview-'+scene;c.app_name=names[scene];c.session='preview-'+(++seq);c.activity=scene==='desktop'?'desktop':'application';
    if(!state.apps.some(p=>p.id===c.app_id)){const sensitive=scene==='sensitive';const observe=!sensitive&&state.settings.automatic_learning;state.apps.push({id:c.app_id,name:c.app_name,observe,volume:observe?'ask':'off',sensitive,explicit:false});}
    if(effective()){state.events.unshift({app_name:c.app_name,kind:scene==='desktop'?'desktop_focus':'application_focus',at:Date.now()/1000});state.event_count++;}
    state.proposal=null;state.app_recommendation=null;
  }else if(name==='demo_volume'){
    state.volume=Number(args.volume);state.proposal=null;
  }else if(name==='capture'){
    throw '网页预览不训练模型；桌面版会在关键事件后自动学习音量变化。';
  }else if(name==='demo_correction'&&c){
    if(!effective()||state.apps.find(p=>p.id===c.app_id)?.sensitive)throw '请切换到普通学习场景';
    const rule={id:'demo-rule-'+(++seq),kind:'volume' as const,scope:{app_id:c.app_id,app_name:c.app_name,device:c.device,period:Math.floor(c.hour/6),activity:c.activity??''},candidates:[],evidence:2,revision:2,last_at:Date.now()/1000,active:true,broad:false,preferred:null,wake:'pending',advice:null,awaiting_confirmation:false};
    state.experience.rules.push(rule);state.experience.pending=[rule];state.proposal=null;state.message='模拟两次独立纠正：本地已学会这个场景不自动调音量；没有调用 API';
  }else if(name==='remove_experience'){
    state.experience.rules=state.experience.rules.filter(e=>e.id!==args.id);state.experience.pending=state.experience.pending.filter(e=>e.id!==args.id);
  }else if(name==='demo_teach'&&c){
    if(!effective()||state.apps.find(p=>p.id===c.app_id)?.volume==='off')throw '敏感软件或已停用软件需要先允许学习与建议';
    state.memories=state.memories.filter(m=>m.app_id!==c.app_id);state.memories.unshift({app_id:c.app_id,name:c.app_name,pinned:false,importance:.2,last_used:Date.now()/1000,days:0,samples:5});
    state.samples=5;state.proposal={id:'preview-'+(++seq),context:structuredClone(c),created:Date.now()/1000,prediction:{target:.35,confidence:.96,similarity:1,feedback_mean:.667,samples:5,sessions:5,reason:'界面预览数据：5 个模拟场景，习惯音量 35%（不参与真实模型）'}};
    state.message='演示建议已加载，未连接系统';
  }else if(name==='accept'){
    const p=state.proposal;if(!p||p.id!==args.id)throw '建议已失效';
    state.journal.unshift({id:'action-'+(++seq),app_name:p.context.app_name,device:p.context.device,before:state.volume!,after:p.prediction.target,at:Date.now()/1000,status:'applied',undoable:true});
    state.volume=p.prediction.target;state.proposal=null;
  }else if(name==='reject'){state.proposal=null;state.message='演示反馈：已拒绝建议';}
  else if(name==='undo'){const e=state.journal.find(e=>e.id===args.id);if(e?.undoable){state.volume=e.before;e.status='undone';e.undoable=false;}}
  else if(name==='demo_recommend'&&c){
    if(!effective()||!state.settings.time_recommendations||state.apps.find(p=>p.id===c.app_id)?.sensitive)throw '请恢复普通场景的学习与时间推荐';
    const id='preview-editor';
    if(c.app_id===id)throw '请先切换到游戏或桌面场景';
    if(!state.apps.some(p=>p.id===id))state.apps.push({id,name:'Editor.exe（模拟）',observe:true,volume:'ask',sensitive:false,explicit:false});
    if(!state.targets.some(t=>t.app_id===id))state.targets.push({kind:'app',app_id:id,name:'Editor.exe（模拟）',enabled:true});
    if(!state.targets.find(t=>t.app_id===id)?.enabled || !state.apps.find(p=>p.id===id)?.observe || state.apps.find(p=>p.id===id)?.sensitive)throw '推荐目标已停用';
    if(!state.memories.some(m=>m.app_id===id))state.memories.push({app_id:id,name:'Editor.exe（模拟）',pinned:false,importance:.403,last_used:Date.now()/1000-7*86400,days:8,samples:0});
    state.app_recommendation={id:'reco-'+(++seq),app_id:id,app_name:'Editor.exe（模拟）',confidence:.792,days:8,reason:'界面预览：8 个独立日期的相近时段使用，已结合星期与近期软件',created:Date.now()/1000};
  }else if(name==='pin_memory'){
    const m=state.memories.find(m=>m.app_id===args.id);if(m){m.pinned=Boolean(args.pinned);m.importance=m.pinned?1:.2;}
  }else if(name==='delete_memory'){
    state.memories=state.memories.filter(m=>m.app_id!==args.id);state.events=state.events.filter(e=>e.app_name!==state.apps.find(p=>p.id===args.id)?.name);state.event_count=state.events.length;state.samples=state.memories.reduce((sum,m)=>sum+m.samples,0);state.proposal=null;state.app_recommendation=null;
  }else if(name==='set_target'){
    const t=state.targets.find(t=>t.app_id===args.id);if(t)t.enabled=Boolean(args.enabled);state.app_recommendation=null;
  }else if(name==='add_target'){throw '网页预览不访问文件；桌面版会打开 exe 选择窗口。';}
  else if(name==='dismiss_app'){state.app_recommendation=null;state.message='演示推荐已忽略';}
  else if(name==='launch_app'){if(!state.app_recommendation||state.app_recommendation.id!==args.id)throw '推荐已失效';state.app_recommendation=null;state.message='模拟打开应用，未启动真实程序';}
  else if(name==='forget'){state.experience={rules:[],pending:[],question:null,local_hits:0,automatic_calls_today:state.experience.automatic_calls_today};state.memories=[];state.app_recommendation=null;state.samples=0;state.events=[];state.event_count=0;state.journal=[];state.proposal=null;state.message='演示记录已清除';}
  state.quick_apps=effective()&&!state.apps.find(p=>p.id===c?.app_id)?.sensitive&&state.settings.time_recommendations ? state.targets.filter(t=>{const p=state.apps.find(p=>p.id===t.app_id);return t.enabled&&p?.observe&&!p.sensitive&&(p.explicit||state.settings.automatic_learning);}).map(t=>{const days=state.memories.find(m=>m.app_id===t.app_id)?.days??0;const overall=1-Math.pow(.99,days);const period=1-Math.pow(.96,days);return {kind:t.kind,app_id:t.app_id,name:t.name,score:.2*overall+.75*period,overall_score:overall,period_score:period,association_score:0,days,period_days:days,active:t.app_id===c?.app_id};}).sort((a,b)=>b.score-a.score||a.name.localeCompare(b.name)).slice(0,6):[];
  return structuredClone(state);
}
