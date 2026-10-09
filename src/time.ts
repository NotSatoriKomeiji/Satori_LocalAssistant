export function greeting(hour:number):string {
  if(hour>=6 && hour<11)return '上午好';
  if(hour>=11 && hour<14)return '中午好';
  if(hour>=14 && hour<18)return '下午好';
  return '晚上好';
}
export function localClock(date=new Date()) {
  return {greeting:greeting(date.getHours()),time:new Intl.DateTimeFormat('zh-CN',{hour:'2-digit',minute:'2-digit',hourCycle:'h23'}).format(date),zone:new Intl.DateTimeFormat('zh-CN',{timeZoneName:'shortOffset'}).formatToParts(date).find(p=>p.type==='timeZoneName')?.value??'本地时间'};
}
