export function siteOrigin(input) {
  try {
    const url=new URL(input);
    if(url.protocol!=='https:'||url.username||url.password||url.port||!url.hostname.includes('.')||/^(?:\d{1,3}\.){3}\d{1,3}$/.test(url.hostname)||url.hostname.endsWith('.')||url.hostname.endsWith('.local')||url.hostname.endsWith('.localhost'))return null;
    if(/(?:^|\/)(?:login|signin|sign-in|oauth|auth|password|credentials|payment|checkout)(?:\/|$)/i.test(url.pathname)||/^(?:login|accounts|passport|auth)\./i.test(url.hostname))return null;
    return url.origin+'/';
  }catch{return null;}
}
