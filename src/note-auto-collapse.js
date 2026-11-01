export function createAutoCollapse({collapse,blocked=()=>false,delay=4000,setTimer=setTimeout,clearTimer=clearTimeout}) {
  let timer=null,inside=false,pinned=false,expanded=false;
  function cancel(){if(timer!==null)clearTimer(timer);timer=null;}
  function schedule(){
    cancel();if(!expanded||inside||pinned)return;
    timer=setTimer(()=>{timer=null;if(!expanded||inside||pinned)return;if(blocked()){schedule();return;}expanded=false;collapse();},delay);
  }
  return {
    enter(){inside=true;cancel();},leave(){inside=false;schedule();},
    activity(){schedule();},revealed(){expanded=true;schedule();},
    collapsed(){expanded=false;cancel();},setPinned(value){pinned=value;schedule();}
  };
}
