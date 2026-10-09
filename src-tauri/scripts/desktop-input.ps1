$ErrorActionPreference='Stop'
[Console]::InputEncoding=[System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,PresentationFramework,PresentationCore,WindowsBase,System.Web.Extensions
$refs=@([System.Diagnostics.Process].Assembly.Location,[System.Windows.Automation.AutomationElement].Assembly.Location,[System.Windows.Automation.TextPatternRangeEndpoint].Assembly.Location,[System.Windows.Window].Assembly.Location,[System.Windows.Media.Brushes].Assembly.Location,[System.Windows.Threading.Dispatcher].Assembly.Location,[System.Linq.Enumerable].Assembly.Location,[System.Web.Script.Serialization.JavaScriptSerializer].Assembly.Location)
Add-Type -ReferencedAssemblies $refs -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Threading;
using System.Web.Script.Serialization;
public static class SatoriInput {
 [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll",EntryPoint="GetWindowLongW")] static extern int GetWindowLong(IntPtr h,int n);
 [DllImport("user32.dll",EntryPoint="SetWindowLongW")] static extern int SetWindowLong(IntPtr h,int n,int v);
 [DllImport("imm32.dll")] static extern IntPtr ImmGetContext(IntPtr h);
 [DllImport("imm32.dll")] static extern bool ImmReleaseContext(IntPtr h,IntPtr c);
 [DllImport("imm32.dll")] static extern int ImmGetCompositionStringW(IntPtr c,uint index,IntPtr data,int length);
 static Dispatcher dispatcher;static Window panel;static StackPanel stack;static AutomationElement target;
 static string before="",learn="",emitted="";static int[] targetId;static string[] words=new string[0];
 static bool permitted=false,inserting=false;static long heartbeat=0,edited=0;static IntPtr foreground;
 static long Clock(){return Stopwatch.GetTimestamp()*1000/Stopwatch.Frequency;}
 static bool Active(){return permitted&&Clock()-heartbeat<1800;}
 static bool Composing(){var h=GetForegroundWindow();var c=ImmGetContext(h);if(c==IntPtr.Zero)return false;try{return ImmGetCompositionStringW(c,8,IntPtr.Zero,0)>0;}finally{ImmReleaseContext(h,c);}}
 static bool Sensitive(string text){text=(text??"").ToLowerInvariant();return new[]{"password","passwd","secret","token","credit","card number","verification","login","sign in","payment","密码","验证码","身份证","银行卡","支付","登录","密钥"}.Any(text.Contains);}
 static bool Eligible(AutomationElement el){
  if(!Active()||el==null||Composing())return false;var c=el.Current;
  if(c.IsPassword||!c.IsEnabled||c.ControlType!=ControlType.Edit||c.ProcessId==Process.GetCurrentProcess().Id||Sensitive(c.Name+" "+c.AutomationId))return false;
  string name=Process.GetProcessById(c.ProcessId).ProcessName.ToLowerInvariant();
  if(new[]{"chrome","msedge","firefox","brave","vivaldi","opera","satori","keepass","keepassxc","1password","bitwarden","credentialuibroker","logonui","consent","cmd","powershell","pwsh","windowsterminal"}.Contains(name))return false;
  var p=el;for(int i=0;i<5&&p!=null;i++){if(Sensitive(p.Current.Name)||p.Current.IsPassword)return false;p=TreeWalker.ControlViewWalker.GetParent(p);}
  object value,text;if(!el.TryGetCurrentPattern(ValuePattern.Pattern,out value)||((ValuePattern)value).Current.IsReadOnly||!el.TryGetCurrentPattern(TextPattern.Pattern,out text))return false;
  var t=(TextPattern)text;var selection=t.GetSelection();if(selection.Length!=1)return false;
  var end=t.DocumentRange.Clone();end.MoveEndpointByRange(TextPatternRangeEndpoint.Start,end,TextPatternRangeEndpoint.End);
  return selection[0].Compare(end);
 }
 static void Clear(){panel.Hide();target=null;learn="";before="";}
 static void Update(bool changed){try{
  if(Composing()){panel.Hide();learn="";return;}
  var el=AutomationElement.FocusedElement;if(!Eligible(el)){Clear();return;}
  var value=((ValuePattern)el.GetCurrentPattern(ValuePattern.Pattern)).Current.Value;if(value.Length>512){Clear();return;}
  var id=el.GetRuntimeId();bool same=target!=null&&id.SequenceEqual(targetId);
  if(!same){panel.Hide();learn="";emitted="";}
  string old=before;target=el;targetId=id;before=value;foreground=GetForegroundWindow();
  if(!changed||!same||old==value||inserting)return;
  edited=Clock();learn=value;
  stack.Children.Clear();stack.Children.Add(new TextBlock{Text="快速词 · 点一下输入",Margin=new Thickness(8),Foreground=Brushes.DimGray});
  foreach(var word in words.Take(6)){string text=word;var b=new Button{Content=new TextBlock{Text=text,TextWrapping=TextWrapping.Wrap},Focusable=false,Margin=new Thickness(4),Padding=new Thickness(8)};b.Click+=(s,e)=>Insert(text);stack.Children.Add(b);}
  if(words.Length==0)stack.Children.Add(new TextBlock{Text="正在学习，常用词句重复出现后会显示在这里。",TextWrapping=TextWrapping.Wrap,Margin=new Thickness(8)});
  panel.Left=Math.Max(0,SystemParameters.WorkArea.Right-panel.Width-16);panel.Top=Math.Max(0,SystemParameters.WorkArea.Bottom-320);panel.Show();
 }catch{Clear();}}
 static void Insert(string text){try{
  if(target==null||!Eligible(target)||Clock()-edited>30000||GetForegroundWindow()!=foreground||!AutomationElement.FocusedElement.GetRuntimeId().SequenceEqual(targetId)) {Clear();return;}
  var value=(ValuePattern)target.GetCurrentPattern(ValuePattern.Pattern);if(value.Current.Value!=before){Clear();return;}
  inserting=true;learn="";value.SetValue(before+text);before=value.Current.Value;emitted=before;panel.Hide();
 }catch{Clear();}finally{inserting=false;}}
 static void Focus(object s,AutomationFocusChangedEventArgs e){dispatcher.BeginInvoke(new Action(()=>Update(false)));}
 static void Changed(object s,AutomationEventArgs e){if(inserting)return;dispatcher.BeginInvoke(new Action(()=>Update(true)));}
 public static void Run(){
 dispatcher=Dispatcher.CurrentDispatcher;stack=new StackPanel();panel=new Window{Title="Satori 快速词",Width=350,MaxHeight=300,SizeToContent=SizeToContent.Height,ShowActivated=false,Focusable=false,Topmost=true,ShowInTaskbar=false,WindowStyle=WindowStyle.ToolWindow,ResizeMode=ResizeMode.NoResize,Background=Brushes.Lavender,Content=stack};
 panel.SourceInitialized+=(s,e)=>{var h=new WindowInteropHelper(panel).Handle;SetWindowLong(h,-20,GetWindowLong(h,-20)|0x08000000);};
 panel.Closing+=(s,e)=>{e.Cancel=true;panel.Hide();};
 var timer=new DispatcherTimer{Interval=TimeSpan.FromMilliseconds(250)};timer.Tick+=(s,e)=>{try{
 if(!Active()||target==null||GetForegroundWindow()!=foreground){Clear();return;}
 if(Composing()){panel.Hide();learn="";return;}
 if(Clock()-edited>30000)panel.Hide();
 if(learn.Length>1&&learn!=emitted&&Clock()-edited>1500&&Eligible(target)&&((ValuePattern)target.GetCurrentPattern(ValuePattern.Pattern)).Current.Value==learn){emitted=learn;Console.WriteLine(new JavaScriptSerializer().Serialize(new {text=learn}));learn="";}
 }catch{Clear();}};timer.Start();
 var reader=new Thread(()=>{try{string line;while((line=Console.ReadLine())!=null){var parsed=new JavaScriptSerializer().DeserializeObject(line) as Dictionary<string,object>;bool allow=(bool)parsed["allowed"];var list=(object[])parsed["words"];var ws=list.Select(x=>(string)((Dictionary<string,object>)x)["text"]).ToArray();dispatcher.BeginInvoke(new Action(()=>{permitted=allow;heartbeat=Clock();words=ws;if(!allow)Clear();}));}}catch{}finally{dispatcher.BeginInvokeShutdown(DispatcherPriority.Normal);}});reader.IsBackground=true;reader.Start();
 Automation.AddAutomationFocusChangedEventHandler(Focus);Automation.AddAutomationEventHandler(TextPattern.TextChangedEvent,AutomationElement.RootElement,TreeScope.Subtree,Changed);
 Console.WriteLine("ready");try{Dispatcher.Run();}finally{Automation.RemoveAllEventHandlers();timer.Stop();}
 }
}
'@
[SatoriInput]::Run()
