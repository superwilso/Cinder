
/* ===== ~Counter  @ 0001d568 ===== */

void __thiscall pst::appmanager::AppManager::Counter::~Counter(Counter *this)

{
  (*(code *)PTR__Counter_00033b0c)(this);
  return;
}



/* ===== StartTimeoutCheck  @ 0001d610 ===== */

void __thiscall pst::appmanager::AppManager::Counter::StartTimeoutCheck(Counter *this)

{
  (*(code *)PTR_StartTimeoutCheck_00033b44)(this);
  return;
}



/* ===== ChangeLifeCycle  @ 0001d694 ===== */

void __thiscall pst::appmanager::AppManager::ChangeLifeCycle(AppManager *this,life_cycle_t *param_1)

{
  (*(code *)PTR_ChangeLifeCycle_00033b70)(this);
  return;
}



/* ===== DoMakeAppForeground  @ 0001d850 ===== */

void __thiscall
pst::appmanager::LifeCycleManager::DoMakeAppForeground(LifeCycleManager *this,AppInfo *param_1)

{
  (*(code *)PTR_DoMakeAppForeground_00033c04)(this);
  return;
}



/* ===== SetInitialized  @ 0001d874 ===== */

void __thiscall
pst::appmanager::AppRegistry::SetInitialized(AppRegistry *this,uint param_1,int param_2)

{
                    /* WARNING: Could not recover jumptable at 0x0001d87c. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)PTR_SetInitialized_00033c10)(this);
  return;
}



/* ===== InitializeApp  @ 0001d994 ===== */

void __thiscall
pst::appmanager::LifeCycleManager::InitializeApp(LifeCycleManager *this,uint param_1,int param_2)

{
  (*(code *)PTR_InitializeApp_00033c70)(this);
  return;
}



/* ===== FireChangeLifeCycle  @ 0001d9d0 ===== */

void pst::services::appmgrserviceBase::FireChangeLifeCycle
               (life_cycle_t *param_1,string *param_2,_func_bool_string_ptr_string_ptr *param_3)

{
  (*(code *)PTR_FireChangeLifeCycle_00033c84)(param_1);
  return;
}



/* ===== StartApp  @ 0001d9f4 ===== */

void __thiscall pst::appmanager::LifeCycleManager::StartApp(LifeCycleManager *this,AppInfo *param_1)

{
  (*(code *)PTR_StartApp_00033c90)(this);
  return;
}



/* ===== StartApp  @ 0001db98 ===== */

void __thiscall
pst::appmanager::AppManagerServiceImpl::StartApp
          (AppManagerServiceImpl *this,StartAppRequest *param_1,Response *param_2)

{
  (*(code *)PTR_StartApp_00033d1c)(this);
  return;
}



/* ===== InitializeComplete  @ 0001dbb0 ===== */

void __thiscall
pst::appmanager::AppManagerServiceImpl::InitializeComplete
          (AppManagerServiceImpl *this,InitializeCompleteRequest *param_1,
          InitializeCompleteResponse *param_2)

{
  (*(code *)PTR_InitializeComplete_00033d24)(this);
  return;
}



/* ===== ChangeLifeCycleComplete  @ 0001dbbc ===== */

void __thiscall
pst::appmanager::AppManagerServiceImpl::ChangeLifeCycleComplete
          (AppManagerServiceImpl *this,ChangeLifeCycleCompleteRequest *param_1,Response *param_2)

{
  (*(code *)PTR_ChangeLifeCycleComplete_00033d28)(this);
  return;
}



/* ===== StartAppBase  @ 0001e000 ===== */

void __thiscall
pst::services::appmgrserviceBase::StartAppBase
          (appmgrserviceBase *this,TransactionParam *param_1,TransactionParam *param_2)

{
                    /* WARNING: Could not recover jumptable at 0x0001e008. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)PTR_StartAppBase_00033e94)(this);
  return;
}



/* ===== InitializeCompleteBase  @ 0001e018 ===== */

void __thiscall
pst::services::appmgrserviceBase::InitializeCompleteBase
          (appmgrserviceBase *this,TransactionParam *param_1,TransactionParam *param_2)

{
  (*(code *)PTR_InitializeCompleteBase_00033e9c)(this);
  return;
}



/* ===== ReadStartAppRequest  @ 0001e090 ===== */

void pst::services::appmgrserviceClient::ReadStartAppRequest
               (TransactionParam *param_1,StartAppRequest *param_2)

{
  (*(code *)PTR_ReadStartAppRequest_00033ec4)(param_1);
  return;
}



/* ===== ReadInitializeCompleteResponse  @ 0001e0cc ===== */

void pst::services::appmgrserviceClient::ReadInitializeCompleteResponse
               (TransactionParam *param_1,InitializeCompleteResponse *param_2)

{
  (*(code *)PTR_ReadInitializeCompleteResponse_00033ed8)(param_1);
  return;
}



/* ===== ReadInitializeCompleteRequest  @ 0001e0d8 ===== */

void pst::services::appmgrserviceClient::ReadInitializeCompleteRequest
               (TransactionParam *param_1,InitializeCompleteRequest *param_2)

{
  (*(code *)PTR_ReadInitializeCompleteRequest_00033edc)(param_1);
  return;
}



/* ===== SizeOfInitializeCompleteResponse  @ 0001e0e4 ===== */

void pst::services::appmgrserviceClient::SizeOfInitializeCompleteResponse
               (InitializeCompleteResponse *param_1)

{
  (*(code *)PTR_SizeOfInitializeCompleteResponse_00033ee0)(param_1);
  return;
}



/* ===== WriteInitializeCompleteResponse  @ 0001e0f0 ===== */

void pst::services::appmgrserviceClient::WriteInitializeCompleteResponse
               (InitializeCompleteResponse *param_1,TransactionParam *param_2)

{
  (*(code *)PTR_WriteInitializeCompleteResponse_00033ee4)(param_1);
  return;
}



/* ===== ReadChangeLifeCycleCompleteRequest  @ 0001e0fc ===== */

void pst::services::appmgrserviceClient::ReadChangeLifeCycleCompleteRequest
               (TransactionParam *param_1,ChangeLifeCycleCompleteRequest *param_2)

{
  (*(code *)PTR_ReadChangeLifeCycleCompleteRequest_00033ee8)(param_1);
  return;
}



/* ===== ChangeLifeCycleBase  @ 0001e288 ===== */

void __thiscall
pst::services::appmgrserviceListenerProxy::ChangeLifeCycleBase
          (appmgrserviceListenerProxy *this,TransactionParam *param_1,TransactionParam *param_2)

{
                    /* WARNING: Could not recover jumptable at 0x0001e290. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)PTR_ChangeLifeCycleBase_00033f6c)(this);
  return;
}



/* ===== SetInitialized  @ 0001fc34 ===== */

/* pst::appmanager::AppRegistry::SetInitialized(unsigned int, int) */

void __thiscall
pst::appmanager::AppRegistry::SetInitialized(AppRegistry *this,uint param_1,int param_2)

{
  int iVar1;
  int iVar2;
  uint uVar3;
  
  iVar1 = **(int **)(DAT_0001fcc4 + 0x1fc48);
  std::mutex::lock();
  uVar3 = (*(int *)(this + 0x7c) - *(int *)(this + 0x78) >> 2) * -0x1642c859;
  if (param_1 <= uVar3 && uVar3 - param_1 != 0) {
    iVar2 = param_1 * 0x5c + *(int *)(this + 0x78);
    *(undefined4 *)(iVar2 + 0x4c) = 1;
    *(int *)(iVar2 + 0x50) = param_2;
    *(undefined4 *)(iVar2 + 0x54) = 3;
    std::condition_variable::notify_one();
    log::Print(8,DAT_0001fcc8 + 0x1fc8c,0x46,DAT_0001fccc + 0x1fc90,DAT_0001fcd0 + 0x1fc92,
               DAT_0001fcd4 + 0x1fc94,param_1,param_1,1);
  }
  std::mutex::unlock();
  iVar1 = **(int **)(DAT_0001fcd8 + 0x1fcb2) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== Initialize  @ 00020c9c ===== */

/* pst::appmanager::AppManager::Initialize(std::string, pst::appmanager::ChangeLifeCycleHandler&,
   std::string*) */

void __thiscall
pst::appmanager::AppManager::Initialize
          (AppManager *this,undefined8 *param_2,undefined4 param_3,byte *param_4)

{
  string *psVar1;
  int *piVar2;
  int *piVar3;
  int *piVar4;
  string *this_00;
  __pid_t _Var5;
  int iVar6;
  Framework *pFVar7;
  code *pcVar8;
  string *this_01;
  int iVar9;
  string *psVar10;
  undefined8 uVar11;
  undefined4 local_c0;
  undefined4 local_bc;
  undefined4 uStack_b8;
  undefined4 local_b4;
  undefined4 uStack_b0;
  undefined4 local_ac;
  undefined4 uStack_a8;
  __pid_t local_a4;
  undefined4 local_a0;
  undefined4 uStack_9c;
  undefined4 uStack_98;
  undefined4 uStack_94;
  string *local_90;
  string *local_8c;
  undefined4 local_88;
  __pid_t local_84;
  vector avStack_7c [12];
  undefined4 local_70;
  undefined4 uStack_6c;
  undefined4 local_68;
  undefined4 local_60;
  undefined4 uStack_5c;
  undefined4 local_58;
  Trace aTStack_50 [16];
  int local_40;
  AppManager *pAStack_3c;
  int *local_30;
  int local_28;
  
  local_28 = **(int **)(DAT_00020f7c + 0x20cb0);
  log::Trace::Trace(aTStack_50,(LogModule *)(DAT_00020f84 + 0x20cc2),
                    (char *)(DAT_00020f88 + 0x20cbe),0xaf,(char *)(DAT_00020f80 + 0x20cba));
  psVar10 = (string *)(this + 4);
  if (((byte)*psVar10 & 1) == 0) {
    *(undefined2 *)psVar10 = 0;
  }
  else {
    **(undefined1 **)(this + 0xc) = 0;
    *(undefined4 *)(this + 8) = 0;
  }
  std::string::reserve((uint)psVar10);
  uVar11 = *param_2;
  *(undefined4 *)(this + 0xc) = *(undefined4 *)(param_2 + 1);
  *(undefined8 *)psVar10 = uVar11;
  *(undefined4 *)param_2 = 0;
  *(undefined4 *)((int)param_2 + 4) = 0;
  *(undefined4 *)(param_2 + 1) = 0;
  *(undefined4 *)(this + 0x10) = param_3;
  psVar1 = (string *)pst::core::Framework::GetReference();
  local_90 = (string *)0x0;
  local_8c = (string *)0x0;
  local_88 = 0;
  std::string::__init((char *)&local_90,DAT_00020f8c + 0x20d1e);
  piVar2 = (int *)pst::core::Framework::GetServiceClient(psVar1);
  std::string::~string((string *)&local_90);
  if ((piVar2 != (int *)0x0) && (*(int *)(this + 0x14) == 0)) {
    local_60 = 0;
    uStack_5c = 0;
    local_58 = 0;
    std::string::__init((char *)&local_60,DAT_00020f90 + 0x20d52);
    local_90 = (string *)0x0;
    local_8c = (string *)0x0;
    local_88 = 0;
    pst::services::binder::ServiceBase::ListenerDescUtil::ParseListenerDescString
              ((string *)&local_60,(vector *)&local_90);
    std::string::~string((string *)&local_60);
    local_70 = 0;
    uStack_6c = 0;
    local_68 = 0;
    std::string::__init((char *)&local_70,DAT_00020f94 + 0x20d84);
    services::binder::ServiceBase::ListenerDescUtil::AddCallingCondition
              ((ListenerDescUtil *)&local_90,(string *)&local_70,psVar10);
    std::string::~string((string *)&local_70);
    piVar3 = operator_new(8);
    *piVar3 = *(int *)(DAT_00020f98 + 0x20da8) + 8;
    piVar3[1] = (int)this;
    piVar4 = *(int **)(this + 0x14);
    *(int **)(this + 0x14) = piVar3;
    if (piVar4 != (int *)0x0) {
      (**(code **)(*piVar4 + 4))();
      piVar3 = *(int **)(this + 0x14);
    }
    pcVar8 = *(code **)(*piVar2 + 0x38);
    pst::services::binder::ServiceBase::ListenerDescUtil::CreateListenerDescString(avStack_7c);
    (*pcVar8)(piVar2,piVar3,avStack_7c);
    std::string::~string((string *)avStack_7c);
    psVar1 = local_90;
    if (local_90 != (string *)0x0) {
      while (local_8c != psVar1) {
        this_01 = local_8c + -0x18;
        this_00 = local_8c + -0xc;
        local_8c = this_01;
        std::string::~string(this_00);
        std::string::~string(this_01);
      }
      operator_delete(local_90);
    }
  }
  _Var5 = getpid();
  std::string::string((string *)&local_90,psVar10);
  uStack_9c = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x4);
  uStack_98 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x8);
  uStack_94 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0xc);
  local_a0 = 0;
  local_84 = _Var5;
  pAStack_3c = this;
  if (piVar2 == (int *)0x0) {
    iVar6 = DAT_00020f9c + 0x20f4e;
    pFVar7 = (Framework *)pst::core::Framework::GetReference();
    local_40 = iVar6;
    local_30 = &local_40;
    core::Framework::SetSignalHandler(pFVar7,&local_40);
    if (local_30 == &local_40) {
      pcVar8 = *(code **)(*local_30 + 0x10);
    }
    else {
      if (local_30 == (int *)0x0) goto LAB_00020f6e;
      pcVar8 = *(code **)(*local_30 + 0x14);
    }
    (*pcVar8)();
    goto LAB_00020f6e;
  }
  psVar10 = (string *)((uint)&local_a0 | 4);
  local_ac = 0;
  uStack_a8 = 0;
  local_bc = 0;
  uStack_b8 = 0;
  local_b4 = 0;
  uStack_b0 = 0;
  std::string::operator=((string *)&uStack_b0,(string *)&local_90);
  local_a4 = local_84;
  iVar6 = (**(code **)(*piVar2 + 0x14))(piVar2,(string *)&uStack_b0,&local_c0);
  switch(local_c0) {
  case 0:
    local_a0 = 0;
    break;
  default:
    local_a0 = 1;
    break;
  case 2:
    local_a0 = 2;
    break;
  case 3:
    local_a0 = 3;
    break;
  case 4:
    local_a0 = 4;
  }
  std::string::operator=(psVar10,(string *)&local_bc);
  std::string::~string((string *)&local_bc);
  std::string::~string((string *)&uStack_b0);
  iVar9 = DAT_00020fa0 + 0x20eac;
  pFVar7 = (Framework *)pst::core::Framework::GetReference();
  local_40 = iVar9;
  local_30 = &local_40;
  core::Framework::SetSignalHandler(pFVar7,&local_40);
  if (local_30 == &local_40) {
    pcVar8 = *(code **)(*local_30 + 0x10);
LAB_00020eca:
    (*pcVar8)();
  }
  else if (local_30 != (int *)0x0) {
    pcVar8 = *(code **)(*local_30 + 0x14);
    goto LAB_00020eca;
  }
  if (iVar6 == 0) {
    if (param_4 != (byte *)0x0) {
      if ((*param_4 & 1) == 0) {
        param_4[0] = 0;
        param_4[1] = 0;
      }
      else {
        **(undefined1 **)(param_4 + 8) = 0;
        param_4[4] = 0;
        param_4[5] = 0;
        param_4[6] = 0;
        param_4[7] = 0;
      }
      std::string::reserve((uint)param_4);
      uVar11 = *(undefined8 *)psVar10;
      *(undefined4 *)(param_4 + 8) = *(undefined4 *)(psVar10 + 8);
      *(undefined8 *)param_4 = uVar11;
      *(undefined4 *)psVar10 = 0;
      *(undefined4 *)(psVar10 + 4) = 0;
      *(undefined4 *)(psVar10 + 8) = 0;
    }
    std::string::~string(psVar10);
    std::string::~string((string *)&local_90);
    log::Trace::~Trace(aTStack_50);
    iVar6 = **(int **)(DAT_00020fac + 0x20f2e) - local_28;
    if (iVar6 == 0) {
      return;
    }
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar6);
  }
LAB_00020f6e:
                    /* WARNING: Subroutine does not return */
  base::AssertFail((char *)(DAT_00020fa4 + 0x20f78),(char *)(DAT_00020fa8 + 0x20f7a),0xe0);
}



/* ===== StartApp  @ 00021040 ===== */

/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */
/* pst::appmanager::AppManager::StartApp(std::string const&) */

void __thiscall pst::appmanager::AppManager::StartApp(AppManager *this,string *param_1)

{
  string *psVar1;
  int *piVar2;
  int iVar3;
  undefined4 local_64;
  undefined4 local_60;
  undefined4 uStack_5c;
  undefined4 local_58;
  undefined4 uStack_54;
  undefined4 local_50;
  undefined4 local_4c;
  string asStack_48 [12];
  undefined4 local_3c;
  undefined4 uStack_38;
  undefined4 local_34;
  Trace aTStack_30 [16];
  int local_20;
  
  local_20 = **(int **)(_DAT_00021130 + 0x21052);
  log::Trace::Trace(aTStack_30,(LogModule *)(DAT_00021138 + 0x2105c),
                    (char *)(DAT_0002113c + 0x2105e),0xe9,(char *)(_LAB_00021134 + 0x21058));
  std::string::string(asStack_48,param_1);
  local_3c = 0;
  uStack_38 = 0;
  local_34 = 0;
  psVar1 = (string *)pst::core::Framework::GetReference();
  local_60 = 0;
  uStack_5c = 0;
  local_58 = 0;
  std::string::__init((char *)&local_60,DAT_00021140 + 0x2108e);
  piVar2 = (int *)pst::core::Framework::GetServiceClient(psVar1);
  std::string::~string((string *)&local_60);
  if (piVar2 != (int *)0x0) {
    uStack_5c = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x4);
    local_58 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x8);
    uStack_54 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0xc);
    local_60 = 0;
    local_50 = 0;
    local_4c = 0;
    std::string::operator=((string *)&local_60,asStack_48);
    std::string::operator=((string *)&uStack_54,(string *)&local_3c);
    iVar3 = (**(code **)(*piVar2 + 0xc))(piVar2,(string *)&local_60,&local_64);
    std::string::~string((string *)&uStack_54);
    std::string::~string((string *)&local_60);
    if (iVar3 == 0) {
      std::string::~string((string *)&local_3c);
      std::string::~string(asStack_48);
      log::Trace::~Trace(aTStack_30);
      iVar3 = **(int **)(DAT_00021144 + 0x2110e) - local_20;
      if (iVar3 != 0) {
                    /* WARNING: Subroutine does not return */
        __stack_chk_fail(iVar3);
      }
      return;
    }
  }
                    /* WARNING: Subroutine does not return */
  base::AssertFail((char *)(DAT_00021148 + 0x2112c),(char *)(DAT_0002114c + 0x2112e),0x101);
}



/* ===== DoStartAppWithParam  @ 00021234 ===== */

/* pst::appmanager::AppManager::DoStartAppWithParam(std::string const&, std::string&&) */

void __thiscall
pst::appmanager::AppManager::DoStartAppWithParam(AppManager *this,string *param_1,string *param_2)

{
  string *psVar1;
  int *piVar2;
  int iVar3;
  undefined4 local_64;
  undefined4 local_60;
  undefined4 uStack_5c;
  undefined4 local_58;
  undefined4 uStack_54;
  undefined4 local_50;
  undefined4 local_4c;
  string asStack_48 [12];
  undefined8 local_3c;
  undefined4 local_34;
  Trace aTStack_30 [16];
  int local_20;
  
  local_20 = **(int **)(DAT_00021338 + 0x21248);
  log::Trace::Trace(aTStack_30,(LogModule *)(DAT_00021340 + 0x21252),
                    (char *)(DAT_00021344 + 0x21254),0x127,(char *)(DAT_0002133c + 0x2124e));
  std::string::string(asStack_48,param_1);
  local_3c = *(undefined8 *)param_2;
  local_34 = *(undefined4 *)(param_2 + 8);
  *(undefined4 *)param_2 = 0;
  *(undefined4 *)(param_2 + 4) = 0;
  *(undefined4 *)(param_2 + 8) = 0;
  psVar1 = (string *)pst::core::Framework::GetReference();
  local_60 = 0;
  uStack_5c = 0;
  local_58 = 0;
  std::string::__init((char *)&local_60,DAT_00021348 + 0x21294);
  piVar2 = (int *)pst::core::Framework::GetServiceClient(psVar1);
  std::string::~string((string *)&local_60);
  if (piVar2 != (int *)0x0) {
    uStack_5c = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x4);
    local_58 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x8);
    uStack_54 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0xc);
    local_60 = 0;
    local_50 = 0;
    local_4c = 0;
    std::string::operator=((string *)&local_60,asStack_48);
    std::string::operator=((string *)&uStack_54,(string *)&local_3c);
    iVar3 = (**(code **)(*piVar2 + 0xc))(piVar2,(string *)&local_60,&local_64);
    std::string::~string((string *)&uStack_54);
    std::string::~string((string *)&local_60);
    if (iVar3 == 0) {
      std::string::~string((string *)&local_3c);
      std::string::~string(asStack_48);
      log::Trace::~Trace(aTStack_30);
      iVar3 = **(int **)(DAT_0002134c + 0x21314) - local_20;
      if (iVar3 != 0) {
                    /* WARNING: Subroutine does not return */
        __stack_chk_fail(iVar3);
      }
      return;
    }
  }
                    /* WARNING: Subroutine does not return */
  base::AssertFail((char *)(DAT_00021350 + 0x21332),(char *)(DAT_00021354 + 0x21334),0x13f);
}



/* ===== ChangeLifeCycle  @ 00021358 ===== */

/* pst::appmanager::AppManager::ChangeLifeCycle(pst::appmanager::life_cycle_t const&) */

void __thiscall pst::appmanager::AppManager::ChangeLifeCycle(AppManager *this,life_cycle_t *param_1)

{
  int iVar1;
  string *psVar2;
  int *piVar3;
  code *pcVar4;
  int local_3c;
  undefined4 local_38;
  undefined4 uStack_34;
  undefined4 local_30;
  undefined4 local_2c;
  string asStack_28 [12];
  undefined4 local_1c;
  int local_18;
  
  local_18 = **(int **)(DAT_00021464 + 0x21368);
  iVar1 = *(int *)param_1;
  if (iVar1 == 0) {
    if (**(int **)(DAT_00021468 + 0x21388) == local_18) {
                    /* WARNING: Could not recover jumptable at 0x0002139a. Too many branches */
                    /* WARNING: Treating indirect jump as call */
      (**(code **)(**(int **)(this + 0x10) + 0x10))();
      return;
    }
    goto LAB_0002143e;
  }
  if (iVar1 == 2) {
    pcVar4 = *(code **)(**(int **)(this + 0x10) + 0xc);
LAB_000213a2:
    (*pcVar4)();
  }
  else if (iVar1 == 3) {
    pcVar4 = *(code **)(**(int **)(this + 0x10) + 8);
    goto LAB_000213a2;
  }
  std::string::string(asStack_28,(string *)(this + 4));
  local_1c = *(undefined4 *)param_1;
  psVar2 = (string *)pst::core::Framework::GetReference();
  local_38 = 0;
  uStack_34 = 0;
  local_30 = 0;
  std::string::__init((char *)&local_38,DAT_0002146c + 0x213c0);
  piVar3 = (int *)pst::core::Framework::GetServiceClient(psVar2);
  std::string::~string((string *)&local_38);
  if (piVar3 != (int *)0x0) {
    local_38 = 0;
    uStack_34 = 0;
    local_30 = 0;
    std::string::operator=((string *)&local_38,asStack_28);
    switch(local_1c) {
    case 0:
      local_2c = 0;
      break;
    case 1:
      local_2c = 1;
      break;
    case 2:
      local_2c = 2;
      break;
    case 3:
      local_2c = 3;
    }
    iVar1 = (**(code **)(*piVar3 + 0x18))(piVar3,(string *)&local_38,&local_3c);
    std::string::~string((string *)&local_38);
    if (iVar1 == 0) {
      if (local_3c != 0) {
                    /* WARNING: Subroutine does not return */
        base::AssertFail((char *)(DAT_00021470 + 0x2145e),(char *)(DAT_00021474 + 0x21460),0x177);
      }
      std::string::~string(asStack_28);
      if (**(int **)(DAT_00021478 + 0x21434) == local_18) {
        return;
      }
LAB_0002143e:
                    /* WARNING: Subroutine does not return */
      __stack_chk_fail();
    }
  }
                    /* WARNING: Subroutine does not return */
  base::AssertFail((char *)(DAT_0002147c + 0x2144e),(char *)(DAT_00021480 + 0x21450),0x176);
}



/* ===== StartTimeoutCheck  @ 00021e7c ===== */

/* pst::appmanager::AppManager::Counter::StartTimeoutCheck() */

void __thiscall pst::appmanager::AppManager::Counter::StartTimeoutCheck(Counter *this)

{
  int iVar1;
  code *pcVar2;
  undefined1 auStack_70 [4];
  int local_6c;
  int aiStack_68 [4];
  int *local_58;
  int aiStack_50 [4];
  int *local_40;
  int local_38;
  Counter *pCStack_34;
  int *local_28;
  int local_20;
  
  local_20 = **(int **)(DAT_00021f38 + 0x21e90);
  std::mutex::lock();
  iVar1 = DAT_00021f40;
  local_38 = DAT_00021f3c;
  this[0x94] = (Counter)0x1;
  local_38 = local_38 + 0x21eb0;
  local_40 = (int *)0x0;
  local_58 = (int *)0x0;
  pCStack_34 = this;
  local_28 = &local_38;
  core::JobQueue::AddJobFunc(auStack_70,iVar1 + 0x21eb6,&local_38,0,aiStack_50,aiStack_68,this);
  if (local_6c != 0) {
    std::__shared_weak_count::__release_shared();
  }
  if (local_58 == aiStack_68) {
    pcVar2 = *(code **)(*local_58 + 0x10);
LAB_00021eee:
    (*pcVar2)();
  }
  else if (local_58 != (int *)0x0) {
    pcVar2 = *(code **)(*local_58 + 0x14);
    goto LAB_00021eee;
  }
  if (local_40 == aiStack_50) {
    pcVar2 = *(code **)(*local_40 + 0x10);
LAB_00021f02:
    (*pcVar2)();
  }
  else if (local_40 != (int *)0x0) {
    pcVar2 = *(code **)(*local_40 + 0x14);
    goto LAB_00021f02;
  }
  if (local_28 == &local_38) {
    pcVar2 = *(code **)(*local_28 + 0x10);
  }
  else {
    if (local_28 == (int *)0x0) goto LAB_00021f18;
    pcVar2 = *(code **)(*local_28 + 0x14);
  }
  (*pcVar2)();
LAB_00021f18:
  std::mutex::unlock();
  if (**(int **)(DAT_00021f44 + 0x21f26) != local_20) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== StopTimeoutCheck  @ 0002207c ===== */

/* pst::appmanager::AppManager::Counter::StopTimeoutCheck() */

void __thiscall pst::appmanager::AppManager::Counter::StopTimeoutCheck(Counter *this)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_000220d0 + 0x2208e);
  std::mutex::lock();
  this[0x94] = (Counter)0x0;
  std::condition_variable::notify_all();
  std::mutex::unlock();
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000220d8 + 0x220b4) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x0002d68c. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_0002d690 + 0x2d694))(DAT_000220d4 + 0x220c6,this);
    return;
  }
  __stack_chk_fail();
}



/* ===== ~Counter  @ 00022178 ===== */

/* pst::appmanager::AppManager::Counter::~Counter() */

void __thiscall pst::appmanager::AppManager::Counter::~Counter(Counter *this)

{
  int iVar1;
  Counter *pCVar2;
  code *pcVar3;
  
  iVar1 = **(int **)(DAT_00022210 + 0x2218a);
  std::mutex::lock();
  this[0x94] = (Counter)0x0;
  std::condition_variable::notify_all();
  std::mutex::unlock();
  core::JobQueue::WaitGroup((JobQueue *)(DAT_00022214 + 0x221b2),this);
  std::condition_variable::~condition_variable((condition_variable *)(this + 0x60));
  pCVar2 = *(Counter **)(this + 0x58);
  if (pCVar2 == this + 0x48) {
    pcVar3 = *(code **)(*(int *)pCVar2 + 0x10);
LAB_000221d0:
    (*pcVar3)();
  }
  else if (pCVar2 != (Counter *)0x0) {
    pcVar3 = *(code **)(*(int *)pCVar2 + 0x14);
    goto LAB_000221d0;
  }
  pCVar2 = *(Counter **)(this + 0x40);
  if (pCVar2 == this + 0x30) {
    pcVar3 = *(code **)(*(int *)pCVar2 + 0x10);
  }
  else {
    if (pCVar2 == (Counter *)0x0) goto LAB_000221ea;
    pcVar3 = *(code **)(*(int *)pCVar2 + 0x14);
  }
  (*pcVar3)();
LAB_000221ea:
  std::mutex::~mutex((mutex *)(this + 0x18));
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00022218 + 0x221f8) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x0002d66c. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_0002d670 + 0x2d674))(this);
    return;
  }
  __stack_chk_fail();
}



/* ===== ChangeLifeCycle  @ 000224f4 ===== */

/* pst::appmanager::AppManager::AppManagerServiceListener::ChangeLifeCycle(pst::services::Iappmgrservice::life_cycle_t
   const&) */

void __thiscall
pst::appmanager::AppManager::AppManagerServiceListener::ChangeLifeCycle
          (AppManagerServiceListener *this,life_cycle_t *param_1)

{
  undefined4 local_10;
  int local_c;
  
  local_c = **(int **)(DAT_00022528 + 0x22500);
  local_10 = *(undefined4 *)param_1;
  AppManager::ChangeLifeCycle(*(AppManager **)(this + 4),(life_cycle_t *)&local_10);
  if (**(int **)(DAT_0002252c + 0x22518) != local_c) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== StartApp  @ 00023eb8 ===== */

/* pst::appmanager::LifeCycleManager::StartApp(pst::appmanager::AppInfo const&) */

void __thiscall pst::appmanager::LifeCycleManager::StartApp(LifeCycleManager *this,AppInfo *param_1)

{
  uint uVar1;
  AppInfo *pAVar2;
  int iVar3;
  AppRegistry *this_00;
  AppInfo *pAVar4;
  uint local_24;
  int local_20;
  
  local_20 = **(int **)(DAT_00024000 + 0x23ecc);
  pAVar4 = *(AppInfo **)(param_1 + 0xc);
  if (((byte)param_1[4] & 1) == 0) {
    pAVar4 = param_1 + 5;
  }
  log::Print(7,DAT_00024008 + 0x23edc,0x1d,DAT_0002400c + 0x23ede,_edata + 0x23ee2,
             DAT_00024010 + 0x23ed6,pAVar4);
  if (*(int *)(param_1 + 0x4c) == 3) {
    pAVar4 = *(AppInfo **)(param_1 + 0xc);
    if (((byte)param_1[4] & 1) == 0) {
      pAVar4 = param_1 + 5;
    }
    log::Print(7,DAT_00024028 + 0x23f16,0x1f,DAT_0002402c + 0x23f1a,DAT_00024024 + 0x23f22,
               DAT_00024030 + 0x23f0c,pAVar4);
  }
  else {
    iVar3 = AppStack::Empty(*(AppStack **)(this + 4));
    if (iVar3 == 0) {
      this_00 = *(AppRegistry **)this;
      uVar1 = AppStack::Get(*(AppStack **)(this + 4),0);
      pAVar4 = (AppInfo *)AppRegistry::GetAppInfoById(this_00,uVar1);
      iVar3 = (**(code **)(**(int **)(this + 8) + 0x10))(*(int **)(this + 8),pAVar4,2);
      if (iVar3 != 0) {
        pAVar2 = *(AppInfo **)(pAVar4 + 0xc);
        if (((byte)pAVar4[4] & 1) == 0) {
          pAVar2 = pAVar4 + 5;
        }
        log::Print(3,DAT_00024018 + 0x23f84,0xbf,DAT_0002401c + 0x23f90,DAT_00024014 + 0x23f80,
                   DAT_00024020 + 0x23f74,pAVar2,iVar3);
        goto LAB_00023fe4;
      }
    }
    else {
      pAVar4 = (AppInfo *)0x0;
    }
    MakeAppForegrond(this,param_1,pAVar4,&local_24);
    AppStack::SetForeground(*(AppStack **)(this + 4),local_24);
    iVar3 = AppRegistry::GetAppInfoById(*(AppRegistry **)this,local_24);
    if (pAVar4 != (AppInfo *)0x0) {
      iVar3 = *(int *)(param_1 + 0x1c);
    }
    if ((pAVar4 != (AppInfo *)0x0 && iVar3 != 2) &&
       (iVar3 = DoTerminateApp(this,pAVar4), iVar3 == 0)) {
      AppStack::Remove(*(AppStack **)(this + 4),*(uint *)pAVar4);
    }
  }
LAB_00023fe4:
  iVar3 = **(int **)(DAT_00024034 + 0x23fec) - local_20;
  if (iVar3 == 0) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar3);
}



/* ===== InitializeApp  @ 0002449c ===== */

/* pst::appmanager::LifeCycleManager::InitializeApp(unsigned int, int) */

void __thiscall
pst::appmanager::LifeCycleManager::InitializeApp(LifeCycleManager *this,uint param_1,int param_2)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_000244f8 + 0x244b0);
  log::Print(7,DAT_00024504 + 0x244c4,0x79,DAT_00024508 + 0x244c0,DAT_00024500 + 0x244be,
             DAT_000244fc + 0x244ba,param_1,param_2);
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_0002450c + 0x244dc) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x0002d6bc. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_0002d6c0 + 0x2d6c4))(*(undefined4 *)this,param_1,param_2);
    return;
  }
  __stack_chk_fail();
}



/* ===== DoMakeAppForeground  @ 00024584 ===== */

/* pst::appmanager::LifeCycleManager::DoMakeAppForeground(pst::appmanager::AppInfo const&) */

void __thiscall
pst::appmanager::LifeCycleManager::DoMakeAppForeground(LifeCycleManager *this,AppInfo *param_1)

{
  int iVar1;
  AppInfo *pAVar2;
  int iVar3;
  undefined4 uVar4;
  int iVar5;
  int iVar6;
  int iVar7;
  undefined1 auStack_18 [4];
  int local_14;
  
  local_14 = **(int **)(DAT_00024638 + 0x24592);
  if (*(int *)(param_1 + 0x4c) == 0) {
    iVar1 = (**(code **)(**(int **)(this + 8) + 8))(*(int **)(this + 8),param_1,auStack_18);
    if (iVar1 == 0) goto LAB_00024620;
    pAVar2 = *(AppInfo **)(param_1 + 0xc);
    iVar7 = DAT_00024658 + 0x245f4;
    if (((byte)param_1[4] & 1) == 0) {
      pAVar2 = param_1 + 5;
    }
    iVar6 = DAT_0002464c + 0x24606;
    iVar3 = DAT_00024650 + 0x2460c;
    iVar5 = DAT_00024654 + 0x24612;
    uVar4 = 0xaa;
  }
  else {
    iVar1 = (**(code **)(**(int **)(this + 8) + 0x10))(*(int **)(this + 8),param_1,3);
    if (iVar1 == 0) goto LAB_00024620;
    iVar7 = DAT_00024648 + 0x245b6;
    pAVar2 = *(AppInfo **)(param_1 + 0xc);
    iVar3 = DAT_00024640 + 0x245c0;
    iVar5 = DAT_00024644 + 0x245c6;
    if (((byte)param_1[4] & 1) == 0) {
      pAVar2 = param_1 + 5;
    }
    uVar4 = 0xb2;
    iVar6 = DAT_0002463c + 0x245ce;
  }
  log::Print(3,iVar3,uVar4,iVar5,iVar6,iVar7,pAVar2,iVar1);
LAB_00024620:
  iVar1 = **(int **)(DAT_0002465c + 0x24628) - local_14;
  if (iVar1 == 0) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar1);
}



/* ===== InvokeAndSetForeground  @ 00024974 ===== */

/* pst::appmanager::LifeCycleController::InvokeAndSetForeground(pst::appmanager::AppInfo const&,
   int*) */

void __thiscall
pst::appmanager::LifeCycleController::InvokeAndSetForeground
          (LifeCycleController *this,AppInfo *param_1,int *param_2)

{
  int iVar1;
  AppInfo *pAVar2;
  AppInfo *pAVar3;
  LifeCycleController *local_80;
  char local_7c;
  int local_30;
  int local_24;
  
  pAVar3 = param_1 + 5;
  local_24 = **(int **)(DAT_00024ad0 + 0x24988);
  pAVar2 = *(AppInfo **)(param_1 + 0xc);
  if (((byte)param_1[4] & 1) == 0) {
    pAVar2 = pAVar3;
  }
  log::Print(8,DAT_00024ad8 + 0x24998,0x3b,DAT_00024adc + 0x249a0,DAT_00024ad4 + 0x249a4,
             DAT_00024ae0 + 0x24994,pAVar2);
  iVar1 = (**(code **)(**(int **)(this + 4) + 8))(*(int **)(this + 4),param_1,param_2);
  if (iVar1 != 0) {
    pAVar2 = *(AppInfo **)(param_1 + 0xc);
    if (((byte)param_1[4] & 1) == 0) {
      pAVar2 = pAVar3;
    }
    log::Print(3,DAT_00024ae8 + 0x249e8,0x41,DAT_00024aec + 0x249ee,DAT_00024ae4 + 0x249e6,
               DAT_00024af0 + 0x249e2,pAVar2,iVar1);
    goto LAB_00024ab2;
  }
  iVar1 = AppRegistry::WaitLifeCycleChanged
                    (*(AppRegistry **)(this + 0xc),*(undefined4 *)param_1,1,
                     *(undefined4 *)(*(int *)(this + 0x10) + 4));
  if (iVar1 == 0) {
    local_80 = this + 0x18;
    local_7c = '\x01';
    std::mutex::lock();
    if (((*(ushort *)(this + 0x14) & 0xff) == 0) || (0xff < *(ushort *)(this + 0x14))) {
LAB_00024a4a:
      std::mutex::unlock();
    }
    else {
      do {
        std::condition_variable::wait((unique_lock *)(this + 0x30));
      } while (this[0x15] == (LifeCycleController)0x0);
      if (local_7c != '\0') goto LAB_00024a4a;
    }
    iVar1 = (**(code **)(*(int *)this + 0x10))(this,param_1,3);
    if (iVar1 == 0) goto LAB_00024ab2;
  }
  pAVar2 = *(AppInfo **)(param_1 + 0xc);
  if (((byte)param_1[4] & 1) == 0) {
    pAVar2 = pAVar3;
  }
  log::Print(3,DAT_00024af8 + 0x24a76,99,DAT_00024afc + 0x24a7c,DAT_00024af4 + 0x24a84,
             DAT_00024b00 + 0x24a6c,pAVar2,iVar1);
  AppInfo::AppInfo((AppInfo *)&local_80,param_1);
  local_30 = *param_2;
  (**(code **)(**(int **)(this + 4) + 0x10))(*(int **)(this + 4),(AppInfo *)&local_80);
  AppInfo::~AppInfo((AppInfo *)&local_80);
LAB_00024ab2:
  iVar1 = **(int **)(DAT_00024b04 + 0x24aba) - local_24;
  if (iVar1 == 0) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar1);
}



/* ===== ChangeLifeCycle  @ 00024bf8 ===== */

/* pst::appmanager::LifeCycleController::ChangeLifeCycle(pst::appmanager::AppInfo const&,
   pst::appmanager::life_cycle_t) */

void __thiscall
pst::appmanager::LifeCycleController::ChangeLifeCycle
          (LifeCycleController *this,undefined4 *param_1,uint param_3)

{
  LifeCycleController LVar1;
  int iVar2;
  int iVar3;
  undefined4 uVar4;
  
  iVar2 = **(int **)(DAT_00024cdc + 0x24c0c);
  iVar3 = param_1[3];
  if ((*(byte *)(param_1 + 1) & 1) == 0) {
    iVar3 = (int)param_1 + 5;
  }
  log::Print(8,DAT_00024ce4 + 0x24c1c,0x81,DAT_00024ce8 + 0x24c24,DAT_00024ce0 + 0x24c28,
             DAT_00024cec + 0x24c18,iVar3,param_3);
  (**(code **)**(undefined4 **)(this + 8))(*(undefined4 **)(this + 8),param_1,param_3);
  uVar4 = **(undefined4 **)(this + 0x10);
  if ((param_3 & 0xfffffffe) == 2) {
    std::mutex::lock();
    LVar1 = this[0x14];
    std::mutex::unlock();
    if (LVar1 != (LifeCycleController)0x0) {
      uVar4 = 0;
    }
  }
  iVar3 = AppRegistry::WaitLifeCycleChanged(*(AppRegistry **)(this + 0xc),*param_1,param_3,uVar4);
  if (iVar3 == 0) {
    (**(code **)(**(int **)(this + 8) + 4))(*(int **)(this + 8),param_1,param_3);
    uVar4 = 0;
  }
  else {
    iVar3 = param_1[3];
    if ((*(byte *)(param_1 + 1) & 1) == 0) {
      iVar3 = (int)param_1 + 5;
    }
    log::Print(3,DAT_00024cf4 + 0x24ca0,0x9a,DAT_00024cf8 + 0x24ca6,DAT_00024cf0 + 0x24c9e,
               DAT_00024cfc + 0x24c9a,iVar3,param_3);
    uVar4 = 4;
  }
  if (**(int **)(DAT_00024d00 + 0x24cca) != iVar2) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(uVar4);
  }
  return;
}



/* ===== StartApp  @ 0002551c ===== */

/* pst::appmanager::AppManagerServiceImpl::StartApp(pst::appmanager::StartAppRequest const&,
   pst::appmanager::Response*) */

void __thiscall
pst::appmanager::AppManagerServiceImpl::StartApp
          (AppManagerServiceImpl *this,StartAppRequest *param_1,Response *param_2)

{
  int iVar1;
  int iVar2;
  undefined4 uVar3;
  code *pcVar4;
  StartAppRequest *pSVar5;
  int iVar6;
  undefined1 auStack_80 [4];
  int local_7c;
  Trace aTStack_78 [16];
  int aiStack_68 [4];
  int *local_58;
  int aiStack_50 [4];
  int *local_40;
  int local_38;
  AppManagerServiceImpl *local_34;
  int *local_28;
  int local_20;
  
  local_20 = **(int **)(DAT_00025668 + 0x25530);
  log::Trace::Trace(aTStack_78,(LogModule *)(DAT_00025670 + 0x2553c),
                    (char *)(DAT_00025674 + 0x2553e),0x8b,(char *)(DAT_0002566c + 0x2553a));
  std::mutex::lock();
  if (*(int *)(this + 0xcc) != 3) {
    *(undefined4 *)param_2 = 1;
    std::mutex::unlock();
    goto LAB_00025646;
  }
  std::mutex::unlock();
  pSVar5 = *(StartAppRequest **)(param_1 + 8);
  if (((byte)*param_1 & 1) == 0) {
    pSVar5 = param_1 + 1;
  }
  iVar1 = AppRegistry::GetAppInfoByName((AppRegistry *)(this + 0x28),(char *)pSVar5);
  if (iVar1 == 0) {
    log::Print(3,DAT_0002567c + 0x255f8,0x98,DAT_00025680 + 0x255fc,DAT_00025684 + 0x255fe,
               DAT_00025688 + 0x25600,pSVar5);
    *(undefined4 *)param_2 = 3;
    goto LAB_00025646;
  }
  std::string::operator=((string *)(this + 0xbc),(string *)(param_1 + 0xc));
  iVar6 = DAT_00025678 + 0x2559a;
  iVar2 = pst::core::Framework::GetReference();
  uVar3 = *(undefined4 *)(iVar2 + 0x30);
  *(int *)(((uint)&local_38 | 4) + 4) = iVar1;
  local_40 = (int *)0x0;
  local_58 = (int *)0x0;
  local_38 = iVar6;
  local_34 = this;
  local_28 = &local_38;
  core::JobQueue::AddJobFunc(auStack_80,uVar3,&local_38,0,aiStack_50,aiStack_68,this + 0xb0);
  if (local_7c != 0) {
    std::__shared_weak_count::__release_shared();
  }
  if (local_58 == aiStack_68) {
    pcVar4 = *(code **)(*local_58 + 0x10);
LAB_00025616:
    (*pcVar4)();
  }
  else if (local_58 != (int *)0x0) {
    pcVar4 = *(code **)(*local_58 + 0x14);
    goto LAB_00025616;
  }
  if (local_40 == aiStack_50) {
    pcVar4 = *(code **)(*local_40 + 0x10);
LAB_0002562a:
    (*pcVar4)();
  }
  else if (local_40 != (int *)0x0) {
    pcVar4 = *(code **)(*local_40 + 0x14);
    goto LAB_0002562a;
  }
  if (local_28 == &local_38) {
    pcVar4 = *(code **)(*local_28 + 0x10);
LAB_0002563e:
    (*pcVar4)();
  }
  else if (local_28 != (int *)0x0) {
    pcVar4 = *(code **)(*local_28 + 0x14);
    goto LAB_0002563e;
  }
  *(undefined4 *)param_2 = 0;
LAB_00025646:
  log::Trace::~Trace(aTStack_78);
  iVar1 = **(int **)(DAT_0002568c + 0x25654) - local_20;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== InitializeComplete  @ 000257c4 ===== */

/* pst::appmanager::AppManagerServiceImpl::InitializeComplete(pst::appmanager::InitializeCompleteRequest
   const&, pst::appmanager::InitializeCompleteResponse*) */

void __thiscall
pst::appmanager::AppManagerServiceImpl::InitializeComplete
          (AppManagerServiceImpl *this,InitializeCompleteRequest *param_1,
          InitializeCompleteResponse *param_2)

{
  InitializeCompleteResponse *pIVar1;
  uint *puVar2;
  int iVar3;
  InitializeCompleteRequest *pIVar4;
  undefined8 uVar5;
  Trace aTStack_2c [16];
  int local_1c;
  
  local_1c = **(int **)(DAT_00025898 + 0x257d8);
  log::Trace::Trace(aTStack_2c,(LogModule *)(DAT_000258a0 + 0x257e4),
                    (char *)(DAT_000258a4 + 0x257e6),0xd8,(char *)(DAT_0002589c + 0x257e2));
  pIVar4 = *(InitializeCompleteRequest **)(param_1 + 8);
  if (((byte)*param_1 & 1) == 0) {
    pIVar4 = param_1 + 1;
  }
  puVar2 = (uint *)AppRegistry::GetAppInfoByName((AppRegistry *)(this + 0x28),(char *)pIVar4);
  if (puVar2 == (uint *)0x0) {
    log::Print(3,DAT_000258a8 + 0x25838,0xdc,DAT_000258ac + 0x2583c,DAT_000258b0 + 0x2583e,
               DAT_000258b4 + 0x25840,pIVar4);
    *(undefined4 *)param_2 = 3;
  }
  else {
    LifeCycleManager::InitializeApp
              ((LifeCycleManager *)(this + 0xb0),*puVar2,*(int *)(param_1 + 0xc));
    *(undefined4 *)param_2 = 0;
    pIVar1 = param_2 + 4;
    if (((byte)*pIVar1 & 1) == 0) {
      *(undefined2 *)pIVar1 = 0;
    }
    else {
      **(undefined1 **)(param_2 + 0xc) = 0;
      *(undefined4 *)(param_2 + 8) = 0;
    }
    std::string::reserve((uint)pIVar1);
    uVar5 = *(undefined8 *)(this + 0xbc);
    *(undefined4 *)(param_2 + 0xc) = *(undefined4 *)(this + 0xc4);
    *(undefined8 *)pIVar1 = uVar5;
    *(undefined4 *)(this + 0xbc) = 0;
    *(undefined4 *)(this + 0xc0) = 0;
    *(undefined4 *)(this + 0xc4) = 0;
  }
  log::Trace::~Trace(aTStack_2c);
  iVar3 = **(int **)(DAT_000258b8 + 0x25886) - local_1c;
  if (iVar3 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar3);
  }
  return;
}



/* ===== ChangeLifeCycleComplete  @ 000258bc ===== */

/* pst::appmanager::AppManagerServiceImpl::ChangeLifeCycleComplete(pst::appmanager::ChangeLifeCycleCompleteRequest
   const&, pst::appmanager::Response*) */

void __thiscall
pst::appmanager::AppManagerServiceImpl::ChangeLifeCycleComplete
          (AppManagerServiceImpl *this,ChangeLifeCycleCompleteRequest *param_1,Response *param_2)

{
  undefined4 *puVar1;
  int iVar2;
  ChangeLifeCycleCompleteRequest *pCVar3;
  undefined4 uVar4;
  Trace aTStack_2c [16];
  int local_1c;
  
  local_1c = **(int **)(DAT_00025954 + 0x258d0);
  log::Trace::Trace(aTStack_2c,(LogModule *)(DAT_0002595c + 0x258dc),
                    (char *)(DAT_00025960 + 0x258de),0xed,(char *)(DAT_00025958 + 0x258da));
  pCVar3 = *(ChangeLifeCycleCompleteRequest **)(param_1 + 8);
  if (((byte)*param_1 & 1) == 0) {
    pCVar3 = param_1 + 1;
  }
  puVar1 = (undefined4 *)AppRegistry::GetAppInfoByName((AppRegistry *)(this + 0x28),(char *)pCVar3);
  if (puVar1 == (undefined4 *)0x0) {
    uVar4 = 3;
    log::Print(3,DAT_00025964 + 0x2591c,0xf1,DAT_00025968 + 0x25920,DAT_0002596c + 0x25922,
               DAT_00025970 + 0x25924,pCVar3);
  }
  else {
    LifeCycleManager::LifeCycleChanged
              ((LifeCycleManager *)(this + 0xb0),*puVar1,*(undefined4 *)(param_1 + 0xc));
    uVar4 = 0;
  }
  *(undefined4 *)param_2 = uVar4;
  log::Trace::~Trace(aTStack_2c);
  iVar2 = **(int **)(DAT_00025974 + 0x25942) - local_1c;
  if (iVar2 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar2);
  }
  return;
}



/* ===== SendChangeLifeCycleEvent  @ 000259f0 ===== */

/* pst::appmanager::AppManagerServiceImpl::SendChangeLifeCycleEvent(pst::appmanager::AppInfo const&,
   pst::appmanager::life_cycle_t) */

void __thiscall
pst::appmanager::AppManagerServiceImpl::SendChangeLifeCycleEvent
          (AppManagerServiceImpl *this,int param_1,undefined4 param_3)

{
  string *psVar1;
  int iVar2;
  string *this_00;
  life_cycle_t *plVar3;
  string *this_01;
  vector avStack_c8 [12];
  undefined4 local_bc;
  undefined4 local_b8;
  undefined4 uStack_b4;
  undefined4 local_b0;
  vector local_ac [12];
  undefined4 local_a0;
  undefined4 uStack_9c;
  undefined4 local_98;
  undefined4 local_90;
  undefined4 uStack_8c;
  undefined4 local_88;
  string *local_84;
  string *local_80;
  undefined4 local_7c;
  char acStack_75 [81];
  int local_24;
  
  local_24 = **(int **)(DAT_00025b54 + 0x25a04);
  log::Print(6,DAT_00025b60 + 0x25a12,0x108,DAT_00025b64 + 0x25a14,DAT_00025b5c + 0x25a18,
             DAT_00025b58 + 0x25a0e,param_3);
  if (*(int *)(this + 200) != 0) {
    local_90 = 0;
    uStack_8c = 0;
    local_88 = 0;
    std::string::__init((char *)&local_90,DAT_00025b68 + 0x25a40);
    local_84 = (string *)0x0;
    local_80 = (string *)0x0;
    local_7c = 0;
    pst::services::binder::ServiceBase::ListenerDescUtil::ParseListenerDescString
              ((string *)&local_90,(vector *)&local_84);
    std::string::~string((string *)&local_90);
    local_a0 = 0;
    uStack_9c = 0;
    local_98 = 0;
    std::string::__init((char *)&local_a0,DAT_00025b6c + 0x25a70);
    services::binder::ServiceBase::ListenerDescUtil::AddCallingCondition
              ((ListenerDescUtil *)&local_84,(string *)&local_a0,(string *)(param_1 + 4));
    std::string::~string((string *)&local_a0);
    pst::services::binder::ServiceBase::ListenerDescUtil::CreateListenerDescString(local_ac);
    FUN_00025b7c(acStack_75);
    std::string::~string((string *)local_ac);
    iVar2 = pst::services::binder::BinderDriver::GetReference();
    local_b8 = 0;
    uStack_b4 = 0;
    local_b0 = 0;
    strlen(acStack_75);
    std::string::__init((char *)&local_b8,(uint)acStack_75);
    pst::services::binder::ServiceManager::AddLog((string *)(iVar2 + 8));
    std::string::~string((string *)&local_b8);
    plVar3 = *(life_cycle_t **)(this + 200);
    local_bc = param_3;
    pst::services::binder::ServiceBase::ListenerDescUtil::CreateListenerDescString(avStack_c8);
    services::appmgrserviceBase::FireChangeLifeCycle
              (plVar3,(string *)&local_bc,(_func_bool_string_ptr_string_ptr *)avStack_c8);
    std::string::~string((string *)avStack_c8);
    psVar1 = local_84;
    if (local_84 != (string *)0x0) {
      while (local_80 != psVar1) {
        this_01 = local_80 + -0x18;
        this_00 = local_80 + -0xc;
        local_80 = this_01;
        std::string::~string(this_00);
        std::string::~string(this_01);
      }
      operator_delete(local_84);
    }
  }
  if (**(int **)(DAT_00025b78 + 0x25b42) == local_24) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== StartApp  @ 000275fc ===== */

/* pst::appmanager::AppManagerService::StartApp(pst::services::Iappmgrservice::StartAppRequest
   const&, pst::services::Iappmgrservice::Response&) */

void __thiscall
pst::appmanager::AppManagerService::StartApp
          (AppManagerService *this,StartAppRequest *param_1,Response *param_2)

{
  StartAppRequest *pSVar1;
  undefined4 uVar2;
  int iVar3;
  StartAppRequest *pSVar4;
  int iVar5;
  int iVar6;
  int iVar7;
  char *pcVar8;
  char *pcVar9;
  LogModule *pLVar10;
  undefined4 local_54;
  undefined4 local_50;
  undefined4 uStack_4c;
  undefined4 uStack_48;
  undefined4 uStack_44;
  undefined4 local_40;
  undefined4 local_3c;
  Trace aTStack_38 [16];
  int local_28;
  
  pcVar9 = (char *)(DAT_0002771c + 0x2761c);
  pcVar8 = (char *)(DAT_00027720 + 0x27622);
  local_28 = **(int **)(DAT_00027710 + 0x27612);
  pLVar10 = (LogModule *)(DAT_00027718 + 0x2762e);
  pSVar1 = *(StartAppRequest **)(param_1 + 0x14);
  pSVar4 = *(StartAppRequest **)(param_1 + 8);
  if (((byte)*(string *)(param_1 + 0xc) & 1) == 0) {
    pSVar1 = param_1 + 0xd;
  }
  if (((byte)*param_1 & 1) == 0) {
    pSVar4 = param_1 + 1;
  }
  log::Print(7,pcVar9,0x96,pcVar8,pLVar10,DAT_00027714 + 0x2762a,pSVar4,pSVar1);
  log::Trace::Trace(aTStack_38,pLVar10,pcVar9,0x98,pcVar8);
  uStack_4c = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x4);
  uStack_48 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x8);
  uStack_44 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0xc);
  local_50 = 0;
  local_40 = 0;
  local_3c = 0;
  std::string::operator=((string *)&local_50,(string *)param_1);
  std::string::operator=((string *)&uStack_44,(string *)(param_1 + 0xc));
  AppManagerServiceImpl::StartApp
            (*(AppManagerServiceImpl **)(this + 100),(StartAppRequest *)&local_50,
             (Response *)&local_54);
  iVar3 = DAT_0002772c;
  switch(local_54) {
  case 0:
    uVar2 = 0;
    break;
  default:
    uVar2 = 1;
    break;
  case 2:
    uVar2 = 2;
    break;
  case 3:
    uVar2 = 3;
    break;
  case 4:
    uVar2 = 4;
  }
  iVar7 = DAT_00027730 + 0x276ca;
  iVar5 = DAT_00027724 + 0x276ce;
  iVar6 = DAT_00027728 + 0x276d0;
  *(undefined4 *)param_2 = uVar2;
  log::Print(7,iVar5,0xaa,iVar6,iVar3 + 0x276d6,iVar7,uVar2);
  std::string::~string((string *)&uStack_44);
  std::string::~string((string *)&local_50);
  log::Trace::~Trace(aTStack_38);
  iVar3 = **(int **)(DAT_00027734 + 0x276fc) - local_28;
  if (iVar3 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar3);
  }
  return;
}



/* ===== InitializeComplete  @ 00027840 ===== */

/* pst::appmanager::AppManagerService::InitializeComplete(pst::services::Iappmgrservice::InitializeCompleteRequest
   const&, pst::services::Iappmgrservice::InitializeCompleteResponse&) */

void __thiscall
pst::appmanager::AppManagerService::InitializeComplete
          (AppManagerService *this,InitializeCompleteRequest *param_1,
          InitializeCompleteResponse *param_2)

{
  InitializeCompleteRequest *pIVar1;
  undefined4 uVar2;
  int iVar3;
  undefined4 uVar4;
  undefined4 local_40;
  undefined4 local_3c;
  undefined4 uStack_38;
  undefined4 local_34;
  undefined4 local_30;
  undefined4 uStack_2c;
  undefined4 local_28;
  undefined4 local_24;
  int local_20;
  
  local_20 = **(int **)(DAT_00027930 + 0x27856);
  pIVar1 = *(InitializeCompleteRequest **)(param_1 + 8);
  uVar4 = *(undefined4 *)(param_1 + 0xc);
  if (((byte)*param_1 & 1) == 0) {
    pIVar1 = param_1 + 1;
  }
  log::Print(7,DAT_00027938 + 0x27862,0xd7,DAT_0002793c + 0x2786a,DAT_00027934 + 0x2786e,
             DAT_00027940 + 0x2785c,pIVar1,uVar4);
  local_30 = 0;
  uStack_2c = 0;
  local_28 = 0;
  std::string::operator=((string *)&local_30,(string *)param_1);
  local_24 = *(undefined4 *)(param_1 + 0xc);
  local_3c = 0;
  uStack_38 = 0;
  local_34 = 0;
  AppManagerServiceImpl::InitializeComplete
            (*(AppManagerServiceImpl **)(this + 100),(InitializeCompleteRequest *)&local_30,
             (InitializeCompleteResponse *)&local_40);
  switch(local_40) {
  case 0:
    uVar2 = 0;
    break;
  default:
    uVar2 = 1;
    break;
  case 2:
    uVar2 = 2;
    break;
  case 3:
    uVar2 = 3;
    break;
  case 4:
    uVar2 = 4;
  }
  *(undefined4 *)param_2 = uVar2;
  std::string::operator=((string *)(param_2 + 4),(string *)&local_3c);
  log::Print(7,DAT_00027948 + 0x278fc,0xea,DAT_0002794c + 0x278fe,DAT_00027944 + 0x278f6,
             DAT_00027950 + 0x278f2,*(undefined4 *)param_2,uVar4);
  std::string::~string((string *)&local_3c);
  std::string::~string((string *)&local_30);
  iVar3 = **(int **)(DAT_00027954 + 0x2791e) - local_20;
  if (iVar3 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar3);
  }
  return;
}



/* ===== ChangeLifeCycleComplete  @ 00027958 ===== */

/* pst::appmanager::AppManagerService::ChangeLifeCycleComplete(pst::services::Iappmgrservice::ChangeLifeCycleCompleteRequest
   const&, pst::services::Iappmgrservice::Response&) */

void __thiscall
pst::appmanager::AppManagerService::ChangeLifeCycleComplete
          (AppManagerService *this,ChangeLifeCycleCompleteRequest *param_1,Response *param_2)

{
  ChangeLifeCycleCompleteRequest *pCVar1;
  undefined4 uVar2;
  int iVar3;
  int iVar4;
  undefined4 uVar5;
  int iVar6;
  int iVar7;
  undefined4 local_34;
  undefined4 local_30;
  undefined4 uStack_2c;
  undefined4 local_28;
  undefined4 local_24;
  int local_20;
  
  local_20 = **(int **)(DAT_00027a44 + 0x2796c);
  pCVar1 = *(ChangeLifeCycleCompleteRequest **)(param_1 + 8);
  uVar5 = *(undefined4 *)(param_1 + 0xc);
  if (((byte)*param_1 & 1) == 0) {
    pCVar1 = param_1 + 1;
  }
  log::Print(7,DAT_00027a4c + 0x27978,0xf8,DAT_00027a50 + 0x27980,DAT_00027a48 + 0x27984,
             DAT_00027a54 + 0x27972,pCVar1,uVar5);
  local_30 = 0;
  uStack_2c = 0;
  local_28 = 0;
  std::string::operator=((string *)&local_30,(string *)param_1);
  switch(*(undefined4 *)(param_1 + 0xc)) {
  case 0:
    local_24 = 0;
    break;
  case 1:
    local_24 = 1;
    break;
  case 2:
    local_24 = 2;
    break;
  case 3:
    local_24 = 3;
  }
  AppManagerServiceImpl::ChangeLifeCycleComplete
            (*(AppManagerServiceImpl **)(this + 100),(ChangeLifeCycleCompleteRequest *)&local_30,
             (Response *)&local_34);
  iVar3 = DAT_00027a60;
  switch(local_34) {
  case 0:
    uVar2 = 0;
    break;
  default:
    uVar2 = 1;
    break;
  case 2:
    uVar2 = 2;
    break;
  case 3:
    uVar2 = 3;
    break;
  case 4:
    uVar2 = 4;
  }
  iVar7 = DAT_00027a64 + 0x27a08;
  iVar4 = DAT_00027a58 + 0x27a0c;
  iVar6 = DAT_00027a5c + 0x27a0e;
  *(undefined4 *)param_2 = uVar2;
  log::Print(7,iVar4,0x10a,iVar6,iVar3 + 0x27a14,iVar7,uVar2,uVar5);
  std::string::~string((string *)&local_30);
  iVar3 = **(int **)(DAT_00027a68 + 0x27a30) - local_20;
  if (iVar3 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar3);
  }
  return;
}



/* ===== FireChangeLifeCycle  @ 000294d8 ===== */

/* pst::services::appmgrserviceBase::FireChangeLifeCycle(pst::services::Iappmgrservice::life_cycle_t
   const&, std::string const&, bool (*)(std::string const&, std::string const&)) */

void pst::services::appmgrserviceBase::FireChangeLifeCycle
               (life_cycle_t *param_1,string *param_2,_func_bool_string_ptr_string_ptr *param_3)

{
  undefined4 *puVar1;
  undefined4 uVar2;
  undefined4 local_70;
  undefined4 uStack_6c;
  undefined4 uStack_68;
  undefined4 uStack_64;
  undefined4 local_60;
  undefined4 uStack_5c;
  undefined4 uStack_58;
  undefined4 uStack_54;
  undefined4 local_50;
  int local_4c;
  undefined4 local_48;
  undefined4 uStack_44;
  undefined4 local_40;
  TimeMeasureHolder aTStack_38 [20];
  int local_24;
  
  local_24 = **(int **)(DAT_000295a4 + 0x294ec);
  local_48 = 0;
  uStack_44 = 0;
  local_40 = 0;
  std::string::__init((char *)&local_48,DAT_000295a8 + 0x294f0);
  pst::services::binder::ServiceManager::TimeMeasureHolder::TimeMeasureHolder
            (aTStack_38,(string *)&local_48,false);
  std::string::~string((string *)&local_48);
  uStack_6c = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x4);
  uStack_68 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x8);
  uStack_64 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0xc);
  local_70 = 0;
  local_60 = 0;
  local_50 = 0;
  local_4c = 0;
  uStack_5c = uStack_6c;
  uStack_58 = uStack_68;
  uStack_54 = uStack_64;
  pst::services::binder::TransactionParam::Init((uint)&local_70,0xc);
  puVar1 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)&local_70);
  if (puVar1 != (undefined4 *)0x0) {
    *puVar1 = 0xffffffff;
  }
  puVar1 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)&local_70);
  if (puVar1 != (undefined4 *)0x0) {
    *puVar1 = 0xffffffff;
  }
  uVar2 = *(undefined4 *)param_2;
  puVar1 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)&local_70);
  if (puVar1 != (undefined4 *)0x0) {
    *puVar1 = uVar2;
  }
  pst::services::binder::ServiceBase::NotifyListeners
            ((uint)(param_1 + 4),(TransactionParam *)0x0,(bool)((char)&stack0xffffffe8 + -0x58),
             (string *)0x1,param_3);
  if (local_4c != 0) {
    std::__shared_weak_count::__release_shared();
  }
  pst::services::binder::ServiceManager::TimeMeasureHolder::~TimeMeasureHolder(aTStack_38);
  if (**(int **)(DAT_000295ac + 0x29594) != local_24) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== StartAppBase  @ 00029a8c ===== */

/* pst::services::appmgrserviceBase::StartAppBase(pst::services::binder::TransactionParam&,
   pst::services::binder::TransactionParam*) */

void __thiscall
pst::services::appmgrserviceBase::StartAppBase
          (appmgrserviceBase *this,TransactionParam *param_1,TransactionParam *param_2)

{
  undefined4 uVar1;
  int iVar2;
  uint uVar3;
  undefined4 *puVar4;
  undefined4 local_60;
  undefined4 uStack_5c;
  undefined4 local_58;
  Response aRStack_54 [4];
  undefined4 local_50;
  undefined4 uStack_4c;
  undefined4 uStack_48;
  undefined4 uStack_44;
  undefined4 local_40;
  undefined4 local_3c;
  TimeMeasureHolder aTStack_34 [20];
  int local_20;
  
  local_20 = **(int **)(DAT_00029b90 + 0x29aa0);
  if (param_2 != (TransactionParam *)0x0) {
    uStack_4c = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x4);
    uStack_48 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x8);
    uStack_44 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0xc);
    local_50 = 0;
    local_40 = 0;
    local_3c = 0;
    appmgrserviceClient::ReadResponse(param_1,aRStack_54);
    appmgrserviceClient::ReadStartAppRequest(param_1,(StartAppRequest *)&local_50);
    if (((byte)param_1[0x18] & 2) == 0) {
      local_60 = 0;
      uStack_5c = 0;
      local_58 = 0;
      std::string::__init((char *)&local_60,DAT_00029b94 + 0x29ae0);
      pst::services::binder::ServiceManager::TimeMeasureHolder::TimeMeasureHolder
                (aTStack_34,(string *)&local_60,false);
      std::string::~string((string *)&local_60);
      uVar1 = (**(code **)(*(int *)this + 0xc))(this,&local_50,aRStack_54);
      iVar2 = appmgrserviceClient::SizeOfResponse(aRStack_54);
      uVar3 = pst::services::binder::TransactionParam::Size();
      if (uVar3 < iVar2 + 8U) {
        pst::services::binder::TransactionParam::Init((uint)param_2,iVar2 + 8U);
      }
      puVar4 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)param_2);
      if (puVar4 != (undefined4 *)0x0) {
        *puVar4 = 0;
      }
      appmgrserviceClient::WriteResponse(aRStack_54,param_2);
      puVar4 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)param_2);
      if (puVar4 != (undefined4 *)0x0) {
        *puVar4 = uVar1;
      }
      pst::services::binder::ServiceManager::TimeMeasureHolder::~TimeMeasureHolder(aTStack_34);
    }
    std::string::~string((string *)&uStack_44);
    std::string::~string((string *)&local_50);
  }
  iVar2 = **(int **)(DAT_00029b98 + 0x29b7c) - local_20;
  if (iVar2 == 0) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar2);
}



/* ===== InitializeCompleteBase  @ 00029c98 ===== */

/* pst::services::appmgrserviceBase::InitializeCompleteBase(pst::services::binder::TransactionParam&,
   pst::services::binder::TransactionParam*) */

void __thiscall
pst::services::appmgrserviceBase::InitializeCompleteBase
          (appmgrserviceBase *this,TransactionParam *param_1,TransactionParam *param_2)

{
  undefined4 uVar1;
  int iVar2;
  uint uVar3;
  undefined4 *puVar4;
  undefined4 local_68;
  undefined4 uStack_64;
  undefined4 local_60;
  InitializeCompleteResponse aIStack_58 [4];
  undefined4 local_54;
  undefined4 uStack_50;
  undefined4 local_4c;
  undefined4 uStack_48;
  undefined4 local_44;
  undefined4 uStack_40;
  TimeMeasureHolder aTStack_38 [20];
  int local_24;
  
  local_24 = **(int **)(DAT_00029d9c + 0x29cac);
  if (param_2 != (TransactionParam *)0x0) {
    local_44 = 0;
    uStack_40 = 0;
    local_54 = 0;
    uStack_50 = 0;
    local_4c = 0;
    uStack_48 = 0;
    appmgrserviceClient::ReadInitializeCompleteResponse(param_1,aIStack_58);
    appmgrserviceClient::ReadInitializeCompleteRequest
              (param_1,(InitializeCompleteRequest *)&uStack_48);
    if (((byte)param_1[0x18] & 2) == 0) {
      local_68 = 0;
      uStack_64 = 0;
      local_60 = 0;
      std::string::__init((char *)&local_68,DAT_00029da0 + 0x29cee);
      pst::services::binder::ServiceManager::TimeMeasureHolder::TimeMeasureHolder
                (aTStack_38,(string *)&local_68,false);
      std::string::~string((string *)&local_68);
      uVar1 = (**(code **)(*(int *)this + 0x14))(this,&uStack_48,aIStack_58);
      iVar2 = appmgrserviceClient::SizeOfInitializeCompleteResponse(aIStack_58);
      uVar3 = pst::services::binder::TransactionParam::Size();
      if (uVar3 < iVar2 + 8U) {
        pst::services::binder::TransactionParam::Init((uint)param_2,iVar2 + 8U);
      }
      puVar4 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)param_2);
      if (puVar4 != (undefined4 *)0x0) {
        *puVar4 = 0;
      }
      appmgrserviceClient::WriteInitializeCompleteResponse(aIStack_58,param_2);
      puVar4 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)param_2);
      if (puVar4 != (undefined4 *)0x0) {
        *puVar4 = uVar1;
      }
      pst::services::binder::ServiceManager::TimeMeasureHolder::~TimeMeasureHolder(aTStack_38);
    }
    std::string::~string((string *)&local_54);
    std::string::~string((string *)&uStack_48);
  }
  iVar2 = **(int **)(DAT_00029da4 + 0x29d8a) - local_24;
  if (iVar2 == 0) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar2);
}



/* ===== ChangeLifeCycleCompleteBase  @ 00029da8 ===== */

/* pst::services::appmgrserviceBase::ChangeLifeCycleCompleteBase(pst::services::binder::TransactionParam&,
   pst::services::binder::TransactionParam*) */

void __thiscall
pst::services::appmgrserviceBase::ChangeLifeCycleCompleteBase
          (appmgrserviceBase *this,TransactionParam *param_1,TransactionParam *param_2)

{
  undefined4 uVar1;
  int iVar2;
  uint uVar3;
  undefined4 *puVar4;
  undefined4 local_50;
  undefined4 uStack_4c;
  undefined4 local_48;
  Response aRStack_44 [4];
  undefined4 local_40;
  undefined4 uStack_3c;
  undefined4 local_38;
  TimeMeasureHolder aTStack_30 [20];
  int local_1c;
  
  local_1c = **(int **)(DAT_00029e98 + 0x29dbc);
  if (param_2 != (TransactionParam *)0x0) {
    local_40 = 0;
    uStack_3c = 0;
    local_38 = 0;
    appmgrserviceClient::ReadResponse(param_1,aRStack_44);
    appmgrserviceClient::ReadChangeLifeCycleCompleteRequest
              (param_1,(ChangeLifeCycleCompleteRequest *)&local_40);
    if (((byte)param_1[0x18] & 2) == 0) {
      local_50 = 0;
      uStack_4c = 0;
      local_48 = 0;
      std::string::__init((char *)&local_50,DAT_00029e9c + 0x29df0);
      pst::services::binder::ServiceManager::TimeMeasureHolder::TimeMeasureHolder
                (aTStack_30,(string *)&local_50,false);
      std::string::~string((string *)&local_50);
      uVar1 = (**(code **)(*(int *)this + 0x18))(this,&local_40,aRStack_44);
      iVar2 = appmgrserviceClient::SizeOfResponse(aRStack_44);
      uVar3 = pst::services::binder::TransactionParam::Size();
      if (uVar3 < iVar2 + 8U) {
        pst::services::binder::TransactionParam::Init((uint)param_2,iVar2 + 8U);
      }
      puVar4 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)param_2);
      if (puVar4 != (undefined4 *)0x0) {
        *puVar4 = 0;
      }
      appmgrserviceClient::WriteResponse(aRStack_44,param_2);
      puVar4 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)param_2);
      if (puVar4 != (undefined4 *)0x0) {
        *puVar4 = uVar1;
      }
      pst::services::binder::ServiceManager::TimeMeasureHolder::~TimeMeasureHolder(aTStack_30);
    }
    std::string::~string((string *)&local_40);
  }
  iVar2 = **(int **)(DAT_00029ea0 + 0x29e84) - local_1c;
  if (iVar2 == 0) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar2);
}



/* ===== ChangeLifeCycleBase  @ 0002a858 ===== */

/* pst::services::appmgrserviceListenerProxy::ChangeLifeCycleBase(pst::services::binder::TransactionParam&,
   pst::services::binder::TransactionParam*) */

void __thiscall
pst::services::appmgrserviceListenerProxy::ChangeLifeCycleBase
          (appmgrserviceListenerProxy *this,TransactionParam *param_1,TransactionParam *param_2)

{
  undefined4 *puVar1;
  int iVar2;
  undefined4 uVar3;
  undefined4 local_40;
  undefined4 uStack_3c;
  undefined4 local_38;
  undefined4 local_34;
  TimeMeasureHolder aTStack_30 [20];
  int local_1c;
  
  local_1c = **(int **)(DAT_0002a8f4 + 0x2a86c);
  if (param_2 == (TransactionParam *)0x0) {
    pst::services::binder::TransactionParam::Get((uint)param_1);
    uVar3 = 0;
    local_34 = 0;
    puVar1 = (undefined4 *)pst::services::binder::TransactionParam::Get((uint)param_1);
    if (puVar1 != (undefined4 *)0x0) {
      uVar3 = *puVar1;
    }
    local_34 = uVar3;
    if (((byte)param_1[0x18] & 2) == 0) {
      local_40 = 0;
      uStack_3c = 0;
      local_38 = 0;
      std::string::__init((char *)&local_40,DAT_0002a8f8 + 0x2a8a6);
      pst::services::binder::ServiceManager::TimeMeasureHolder::TimeMeasureHolder
                (aTStack_30,(string *)&local_40,false);
      std::string::~string((string *)&local_40);
      (**(code **)(**(int **)(this + 0x24) + 8))(*(int **)(this + 0x24),&local_34);
      pst::services::binder::ServiceManager::TimeMeasureHolder::~TimeMeasureHolder(aTStack_30);
    }
  }
  iVar2 = **(int **)(DAT_0002a8fc + 0x2a8e0) - local_1c;
  if (iVar2 == 0) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar2);
}



/* ===== StartApp  @ 0002aca8 ===== */

/* pst::services::appmgrserviceClient::StartApp(pst::services::Iappmgrservice::StartAppRequest
   const&, pst::services::Iappmgrservice::Response&) */

void __thiscall
pst::services::appmgrserviceClient::StartApp
          (appmgrserviceClient *this,StartAppRequest *param_1,Response *param_2)

{
  int iVar1;
  uint uVar2;
  undefined4 *puVar3;
  uint uVar4;
  undefined4 uVar5;
  undefined4 local_98;
  undefined4 uStack_94;
  uint uStack_90;
  undefined4 uStack_8c;
  undefined4 local_88;
  undefined4 uStack_84;
  uint local_80;
  undefined4 uStack_7c;
  undefined4 local_78;
  undefined4 uStack_74;
  char local_70 [4];
  undefined4 uStack_6c;
  uint uStack_68;
  undefined4 uStack_64;
  undefined4 local_60;
  undefined4 uStack_5c;
  uint local_58;
  undefined4 uStack_54;
  undefined4 local_50;
  int local_4c;
  int *local_48;
  int local_44;
  undefined4 local_40;
  undefined4 uStack_3c;
  undefined4 local_38;
  TimeMeasureHolder aTStack_34 [20];
  int local_20;
  
  local_20 = **(int **)(DAT_0002aea8 + 0x2acbc);
  local_40 = 0;
  uStack_3c = 0;
  local_38 = 0;
  std::string::__init((char *)&local_40,DAT_0002aeac + 0x2acc0);
  pst::services::binder::ServiceManager::TimeMeasureHolder::TimeMeasureHolder
            (aTStack_34,(string *)&local_40,false);
  std::string::~string((string *)&local_40);
  iVar1 = pst::services::binder::BinderDriver::GetReference();
  pst::services::binder::BinderDriver::SetLastError(iVar1);
  pst::services::binder::ServiceClientBase::GetService();
  if (local_48 != (int *)0x0) {
    uStack_94 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x4);
    uStack_90 = *(uint *)((undefined1  [16])0x0 + (undefined1  [16])0x8);
    uStack_8c = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0xc);
    local_70[0] = '\0';
    local_70[1] = '\0';
    local_70[2] = '\0';
    local_70[3] = '\0';
    local_60 = 0;
    local_50 = 0;
    local_4c = 0;
    local_98 = 0;
    local_88 = 0;
    local_78 = 0;
    uStack_74 = 0;
    uVar2 = *(uint *)(param_1 + 0x10);
    uVar4 = *(uint *)(param_1 + 4);
    if (((byte)param_1[0xc] & 1) == 0) {
      uVar2 = (uint)((byte)param_1[0xc] >> 1);
    }
    if (((byte)*param_1 & 1) == 0) {
      uVar4 = (uint)((byte)*param_1 >> 1);
    }
    uStack_84 = uStack_94;
    local_80 = uStack_90;
    uStack_7c = uStack_8c;
    uStack_6c = uStack_94;
    uStack_68 = uStack_90;
    uStack_64 = uStack_8c;
    uStack_5c = uStack_94;
    local_58 = uStack_90;
    uStack_54 = uStack_8c;
    pst::services::binder::TransactionParam::Init
              ((uint)local_70,(uVar4 + 0x13 & 0xfffffffc) + (uVar2 + 3 & 0xfffffffc));
    puVar3 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)local_70);
    if (puVar3 != (undefined4 *)0x0) {
      *puVar3 = 0;
    }
    uVar5 = *(undefined4 *)param_2;
    puVar3 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)local_70);
    if (puVar3 != (undefined4 *)0x0) {
      *puVar3 = uVar5;
    }
    pst::services::binder::TransactionParam::PutStr(local_70);
    pst::services::binder::TransactionParam::PutStr(local_70);
    if ((local_58 & 2) == 0) {
      iVar1 = (**(code **)(*local_48 + 0x10))();
      if (iVar1 == 0) {
        pst::services::binder::TransactionParam::ResetPosition();
      }
      iVar1 = pst::services::binder::BinderDriver::GetReference();
      (**(code **)(*local_48 + 0x20))(local_48,10,local_70,&local_98);
      pst::services::binder::BinderDriver::SetLastError(iVar1);
      pst::services::binder::BinderDriver::GetReference();
      iVar1 = pst::services::binder::BinderDriver::GetLastError();
      if (iVar1 == 0) {
        iVar1 = (**(code **)(*local_48 + 0x10))();
        if (iVar1 == 0) {
          pst::services::binder::TransactionParam::ResetPosition();
        }
        iVar1 = pst::services::binder::BinderDriver::GetReference();
        pst::services::binder::TransactionParam::Get((uint)&local_98);
        pst::services::binder::BinderDriver::SetLastError(iVar1);
        pst::services::binder::BinderDriver::GetReference();
        iVar1 = pst::services::binder::BinderDriver::GetLastError();
        if (iVar1 == 0) {
          puVar3 = (undefined4 *)pst::services::binder::TransactionParam::Get((uint)&local_98);
          if (puVar3 == (undefined4 *)0x0) {
            uVar5 = 0;
          }
          else {
            uVar5 = *puVar3;
          }
          *(undefined4 *)param_2 = uVar5;
          pst::services::binder::TransactionParam::Get((uint)&local_98);
          if ((local_80 & 2) != 0) {
            iVar1 = pst::services::binder::BinderDriver::GetReference();
            pst::services::binder::BinderDriver::SetLastError(iVar1);
          }
        }
      }
    }
    else {
      iVar1 = pst::services::binder::BinderDriver::GetReference();
      pst::services::binder::BinderDriver::SetLastError(iVar1);
    }
    pst::services::binder::TransactionParamAutoFree::~TransactionParamAutoFree
              ((TransactionParamAutoFree *)&local_98);
    if (local_4c != 0) {
      std::__shared_weak_count::__release_shared();
    }
  }
  if (local_44 != 0) {
    std::__shared_weak_count::__release_shared();
  }
  pst::services::binder::ServiceManager::TimeMeasureHolder::~TimeMeasureHolder(aTStack_34);
  iVar1 = **(int **)(DAT_0002aeb0 + 0x2ae94) - local_20;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== SizeOfStartAppRequest  @ 0002aeb4 ===== */

/* pst::services::appmgrserviceClient::SizeOfStartAppRequest(pst::services::Iappmgrservice::StartAppRequest
   const&) */

void pst::services::appmgrserviceClient::SizeOfStartAppRequest(StartAppRequest *param_1)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_0002af08 + 0x2aee4) - **(int **)(DAT_0002af04 + 0x2aec0);
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== WriteStartAppRequest  @ 0002af84 ===== */

/* pst::services::appmgrserviceClient::WriteStartAppRequest(pst::services::Iappmgrservice::StartAppRequest
   const&, pst::services::binder::TransactionParam&) */

void pst::services::appmgrserviceClient::WriteStartAppRequest
               (StartAppRequest *param_1,TransactionParam *param_2)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_0002afdc + 0x2af94);
  pst::services::binder::TransactionParam::PutStr((char *)param_2);
  pst::services::binder::TransactionParam::PutStr((char *)param_2);
  if (**(int **)(DAT_0002afe0 + 0x2afc6) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail((*(uint *)(param_2 + 0x18) & 3) >> 1);
  }
  return;
}



/* ===== InitializeComplete  @ 0002b29c ===== */

/* pst::services::appmgrserviceClient::InitializeComplete(pst::services::Iappmgrservice::InitializeCompleteRequest
   const&, pst::services::Iappmgrservice::InitializeCompleteResponse&) */

void __thiscall
pst::services::appmgrserviceClient::InitializeComplete
          (appmgrserviceClient *this,InitializeCompleteRequest *param_1,
          InitializeCompleteResponse *param_2)

{
  int iVar1;
  uint uVar2;
  undefined4 *puVar3;
  uint uVar4;
  undefined4 uVar5;
  undefined4 local_98;
  undefined4 uStack_94;
  uint uStack_90;
  undefined4 uStack_8c;
  undefined4 local_88;
  undefined4 uStack_84;
  uint local_80;
  undefined4 uStack_7c;
  undefined4 local_78;
  undefined4 uStack_74;
  char local_70 [4];
  undefined4 uStack_6c;
  uint uStack_68;
  undefined4 uStack_64;
  undefined4 local_60;
  undefined4 uStack_5c;
  uint local_58;
  undefined4 uStack_54;
  undefined4 local_50;
  int local_4c;
  int *local_48;
  int local_44;
  undefined4 local_40;
  undefined4 uStack_3c;
  undefined4 local_38;
  TimeMeasureHolder aTStack_34 [20];
  int local_20;
  
  local_20 = **(int **)(DAT_0002b4c0 + 0x2b2b0);
  local_40 = 0;
  uStack_3c = 0;
  local_38 = 0;
  std::string::__init((char *)&local_40,DAT_0002b4c4 + 0x2b2b4);
  pst::services::binder::ServiceManager::TimeMeasureHolder::TimeMeasureHolder
            (aTStack_34,(string *)&local_40,false);
  std::string::~string((string *)&local_40);
  iVar1 = pst::services::binder::BinderDriver::GetReference();
  pst::services::binder::BinderDriver::SetLastError(iVar1);
  pst::services::binder::ServiceClientBase::GetService();
  if (local_48 != (int *)0x0) {
    uStack_94 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x4);
    uStack_90 = *(uint *)((undefined1  [16])0x0 + (undefined1  [16])0x8);
    uStack_8c = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0xc);
    local_70[0] = '\0';
    local_70[1] = '\0';
    local_70[2] = '\0';
    local_70[3] = '\0';
    local_60 = 0;
    local_50 = 0;
    local_4c = 0;
    local_98 = 0;
    local_88 = 0;
    local_78 = 0;
    uStack_74 = 0;
    uVar2 = *(uint *)(param_1 + 4);
    uVar4 = *(uint *)(param_2 + 8);
    if (((byte)*param_1 & 1) == 0) {
      uVar2 = (uint)((byte)*param_1 >> 1);
    }
    if (((byte)param_2[4] & 1) == 0) {
      uVar4 = (uint)((byte)param_2[4] >> 1);
    }
    uStack_84 = uStack_94;
    local_80 = uStack_90;
    uStack_7c = uStack_8c;
    uStack_6c = uStack_94;
    uStack_68 = uStack_90;
    uStack_64 = uStack_8c;
    uStack_5c = uStack_94;
    local_58 = uStack_90;
    uStack_54 = uStack_8c;
    pst::services::binder::TransactionParam::Init
              ((uint)local_70,(uVar4 + 0xb & 0xfffffffc) + (uVar2 + 0xf & 0xfffffffc));
    puVar3 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)local_70);
    if (puVar3 != (undefined4 *)0x0) {
      *puVar3 = 2;
    }
    uVar5 = *(undefined4 *)param_2;
    puVar3 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)local_70);
    if (puVar3 != (undefined4 *)0x0) {
      *puVar3 = uVar5;
    }
    pst::services::binder::TransactionParam::PutStr(local_70);
    pst::services::binder::TransactionParam::PutStr(local_70);
    puVar3 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)local_70);
    if (puVar3 != (undefined4 *)0x0) {
      *puVar3 = *(undefined4 *)(param_1 + 0xc);
    }
    if ((local_58 & 2) == 0) {
      iVar1 = (**(code **)(*local_48 + 0x10))();
      if (iVar1 == 0) {
        pst::services::binder::TransactionParam::ResetPosition();
      }
      iVar1 = pst::services::binder::BinderDriver::GetReference();
      (**(code **)(*local_48 + 0x20))(local_48,10,local_70,&local_98);
      pst::services::binder::BinderDriver::SetLastError(iVar1);
      pst::services::binder::BinderDriver::GetReference();
      iVar1 = pst::services::binder::BinderDriver::GetLastError();
      if (iVar1 == 0) {
        iVar1 = (**(code **)(*local_48 + 0x10))();
        if (iVar1 == 0) {
          pst::services::binder::TransactionParam::ResetPosition();
        }
        iVar1 = pst::services::binder::BinderDriver::GetReference();
        pst::services::binder::TransactionParam::Get((uint)&local_98);
        pst::services::binder::BinderDriver::SetLastError(iVar1);
        pst::services::binder::BinderDriver::GetReference();
        iVar1 = pst::services::binder::BinderDriver::GetLastError();
        if (iVar1 == 0) {
          puVar3 = (undefined4 *)pst::services::binder::TransactionParam::Get((uint)&local_98);
          if (puVar3 == (undefined4 *)0x0) {
            uVar5 = 0;
          }
          else {
            uVar5 = *puVar3;
          }
          *(undefined4 *)param_2 = uVar5;
          pst::services::binder::TransactionParam::GetStr((string *)&local_98);
          pst::services::binder::TransactionParam::Get((uint)&local_98);
          if ((local_80 & 2) != 0) {
            iVar1 = pst::services::binder::BinderDriver::GetReference();
            pst::services::binder::BinderDriver::SetLastError(iVar1);
          }
        }
      }
    }
    else {
      iVar1 = pst::services::binder::BinderDriver::GetReference();
      pst::services::binder::BinderDriver::SetLastError(iVar1);
    }
    pst::services::binder::TransactionParamAutoFree::~TransactionParamAutoFree
              ((TransactionParamAutoFree *)&local_98);
    if (local_4c != 0) {
      std::__shared_weak_count::__release_shared();
    }
  }
  if (local_44 != 0) {
    std::__shared_weak_count::__release_shared();
  }
  pst::services::binder::ServiceManager::TimeMeasureHolder::~TimeMeasureHolder(aTStack_34);
  iVar1 = **(int **)(DAT_0002b4c8 + 0x2b4ae) - local_20;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== SizeOfInitializeCompleteRequest  @ 0002b4cc ===== */

/* pst::services::appmgrserviceClient::SizeOfInitializeCompleteRequest(pst::services::Iappmgrservice::InitializeCompleteRequest
   const&) */

void pst::services::appmgrserviceClient::SizeOfInitializeCompleteRequest
               (InitializeCompleteRequest *param_1)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_0002b50c + 0x2b4f0) - **(int **)(DAT_0002b508 + 0x2b4d8);
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== SizeOfInitializeCompleteResponse  @ 0002b510 ===== */

/* pst::services::appmgrserviceClient::SizeOfInitializeCompleteResponse(pst::services::Iappmgrservice::InitializeCompleteResponse
   const&) */

void pst::services::appmgrserviceClient::SizeOfInitializeCompleteResponse
               (InitializeCompleteResponse *param_1)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_0002b550 + 0x2b534) - **(int **)(DAT_0002b54c + 0x2b51c);
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== WriteInitializeCompleteResponse  @ 0002b554 ===== */

/* pst::services::appmgrserviceClient::WriteInitializeCompleteResponse(pst::services::Iappmgrservice::InitializeCompleteResponse
   const&, pst::services::binder::TransactionParam&) */

void pst::services::appmgrserviceClient::WriteInitializeCompleteResponse
               (InitializeCompleteResponse *param_1,TransactionParam *param_2)

{
  int iVar1;
  undefined4 *puVar2;
  undefined4 uVar3;
  
  iVar1 = **(int **)(DAT_0002b5a8 + 0x2b566);
  uVar3 = *(undefined4 *)param_1;
  puVar2 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)param_2);
  if (puVar2 != (undefined4 *)0x0) {
    *puVar2 = uVar3;
  }
  pst::services::binder::TransactionParam::PutStr((char *)param_2);
  if (**(int **)(DAT_0002b5ac + 0x2b592) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail((*(uint *)(param_2 + 0x18) & 3) >> 1);
  }
  return;
}



/* ===== WriteInitializeCompleteRequest  @ 0002b5b0 ===== */

/* pst::services::appmgrserviceClient::WriteInitializeCompleteRequest(pst::services::Iappmgrservice::InitializeCompleteRequest
   const&, pst::services::binder::TransactionParam&) */

void pst::services::appmgrserviceClient::WriteInitializeCompleteRequest
               (InitializeCompleteRequest *param_1,TransactionParam *param_2)

{
  int iVar1;
  undefined4 *puVar2;
  
  iVar1 = **(int **)(DAT_0002b604 + 0x2b5c0);
  pst::services::binder::TransactionParam::PutStr((char *)param_2);
  puVar2 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)param_2);
  if (puVar2 != (undefined4 *)0x0) {
    *puVar2 = *(undefined4 *)(param_1 + 0xc);
  }
  if (**(int **)(DAT_0002b608 + 0x2b5ee) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail((*(uint *)(param_2 + 0x18) & 3) >> 1);
  }
  return;
}



/* ===== ReadInitializeCompleteResponse  @ 0002b60c ===== */

/* pst::services::appmgrserviceClient::ReadInitializeCompleteResponse(pst::services::binder::TransactionParam&,
   pst::services::Iappmgrservice::InitializeCompleteResponse&) */

void pst::services::appmgrserviceClient::ReadInitializeCompleteResponse
               (TransactionParam *param_1,InitializeCompleteResponse *param_2)

{
  int iVar1;
  undefined4 *puVar2;
  undefined4 uVar3;
  
  iVar1 = **(int **)(DAT_0002b658 + 0x2b61e);
  puVar2 = (undefined4 *)pst::services::binder::TransactionParam::Get((uint)param_1);
  if (puVar2 == (undefined4 *)0x0) {
    uVar3 = 0;
  }
  else {
    uVar3 = *puVar2;
  }
  *(undefined4 *)param_2 = uVar3;
  pst::services::binder::TransactionParam::GetStr((string *)param_1);
  if (**(int **)(DAT_0002b65c + 0x2b644) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(*(undefined4 *)(param_1 + 0x18));
  }
  return;
}



/* ===== ChangeLifeCycleComplete  @ 0002b660 ===== */

/* pst::services::appmgrserviceClient::ChangeLifeCycleComplete(pst::services::Iappmgrservice::ChangeLifeCycleCompleteRequest
   const&, pst::services::Iappmgrservice::Response&) */

void __thiscall
pst::services::appmgrserviceClient::ChangeLifeCycleComplete
          (appmgrserviceClient *this,ChangeLifeCycleCompleteRequest *param_1,Response *param_2)

{
  int iVar1;
  uint uVar2;
  undefined4 *puVar3;
  undefined4 uVar4;
  undefined4 local_98;
  undefined4 uStack_94;
  uint uStack_90;
  undefined4 uStack_8c;
  undefined4 local_88;
  undefined4 uStack_84;
  uint local_80;
  undefined4 uStack_7c;
  undefined4 local_78;
  undefined4 uStack_74;
  char local_70 [4];
  undefined4 uStack_6c;
  uint uStack_68;
  undefined4 uStack_64;
  undefined4 local_60;
  undefined4 uStack_5c;
  uint local_58;
  undefined4 uStack_54;
  undefined4 local_50;
  int local_4c;
  int *local_48;
  int local_44;
  undefined4 local_40;
  undefined4 uStack_3c;
  undefined4 local_38;
  TimeMeasureHolder aTStack_34 [20];
  int local_20;
  
  local_20 = **(int **)(DAT_0002b844 + 0x2b674);
  local_40 = 0;
  uStack_3c = 0;
  local_38 = 0;
  std::string::__init((char *)&local_40,DAT_0002b848 + 0x2b678);
  pst::services::binder::ServiceManager::TimeMeasureHolder::TimeMeasureHolder
            (aTStack_34,(string *)&local_40,false);
  std::string::~string((string *)&local_40);
  iVar1 = pst::services::binder::BinderDriver::GetReference();
  pst::services::binder::BinderDriver::SetLastError(iVar1);
  pst::services::binder::ServiceClientBase::GetService();
  if (local_48 != (int *)0x0) {
    uStack_94 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x4);
    uStack_90 = *(uint *)((undefined1  [16])0x0 + (undefined1  [16])0x8);
    uStack_8c = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0xc);
    local_70[0] = '\0';
    local_70[1] = '\0';
    local_70[2] = '\0';
    local_70[3] = '\0';
    local_60 = 0;
    local_50 = 0;
    local_4c = 0;
    local_98 = 0;
    local_88 = 0;
    local_78 = 0;
    uStack_74 = 0;
    uVar2 = *(uint *)(param_1 + 4);
    if (((byte)*param_1 & 1) == 0) {
      uVar2 = (uint)((byte)*param_1 >> 1);
    }
    uStack_84 = uStack_94;
    local_80 = uStack_90;
    uStack_7c = uStack_8c;
    uStack_6c = uStack_94;
    uStack_68 = uStack_90;
    uStack_64 = uStack_8c;
    uStack_5c = uStack_94;
    local_58 = uStack_90;
    uStack_54 = uStack_8c;
    pst::services::binder::TransactionParam::Init((uint)local_70,uVar2 + 0x13 & 0xfffffffc);
    puVar3 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)local_70);
    if (puVar3 != (undefined4 *)0x0) {
      *puVar3 = 3;
    }
    uVar4 = *(undefined4 *)param_2;
    puVar3 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)local_70);
    if (puVar3 != (undefined4 *)0x0) {
      *puVar3 = uVar4;
    }
    pst::services::binder::TransactionParam::PutStr(local_70);
    uVar4 = *(undefined4 *)(param_1 + 0xc);
    puVar3 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)local_70);
    if (puVar3 != (undefined4 *)0x0) {
      *puVar3 = uVar4;
    }
    if ((local_58 & 2) == 0) {
      iVar1 = (**(code **)(*local_48 + 0x10))();
      if (iVar1 == 0) {
        pst::services::binder::TransactionParam::ResetPosition();
      }
      iVar1 = pst::services::binder::BinderDriver::GetReference();
      (**(code **)(*local_48 + 0x20))(local_48,10,local_70,&local_98);
      pst::services::binder::BinderDriver::SetLastError(iVar1);
      pst::services::binder::BinderDriver::GetReference();
      iVar1 = pst::services::binder::BinderDriver::GetLastError();
      if (iVar1 == 0) {
        iVar1 = (**(code **)(*local_48 + 0x10))();
        if (iVar1 == 0) {
          pst::services::binder::TransactionParam::ResetPosition();
        }
        iVar1 = pst::services::binder::BinderDriver::GetReference();
        pst::services::binder::TransactionParam::Get((uint)&local_98);
        pst::services::binder::BinderDriver::SetLastError(iVar1);
        pst::services::binder::BinderDriver::GetReference();
        iVar1 = pst::services::binder::BinderDriver::GetLastError();
        if (iVar1 == 0) {
          puVar3 = (undefined4 *)pst::services::binder::TransactionParam::Get((uint)&local_98);
          if (puVar3 == (undefined4 *)0x0) {
            uVar4 = 0;
          }
          else {
            uVar4 = *puVar3;
          }
          *(undefined4 *)param_2 = uVar4;
          pst::services::binder::TransactionParam::Get((uint)&local_98);
          if ((local_80 & 2) != 0) {
            iVar1 = pst::services::binder::BinderDriver::GetReference();
            pst::services::binder::BinderDriver::SetLastError(iVar1);
          }
        }
      }
    }
    else {
      iVar1 = pst::services::binder::BinderDriver::GetReference();
      pst::services::binder::BinderDriver::SetLastError(iVar1);
    }
    pst::services::binder::TransactionParamAutoFree::~TransactionParamAutoFree
              ((TransactionParamAutoFree *)&local_98);
    if (local_4c != 0) {
      std::__shared_weak_count::__release_shared();
    }
  }
  if (local_44 != 0) {
    std::__shared_weak_count::__release_shared();
  }
  pst::services::binder::ServiceManager::TimeMeasureHolder::~TimeMeasureHolder(aTStack_34);
  iVar1 = **(int **)(DAT_0002b84c + 0x2b830) - local_20;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== SizeOfChangeLifeCycleCompleteRequest  @ 0002b850 ===== */

/* pst::services::appmgrserviceClient::SizeOfChangeLifeCycleCompleteRequest(pst::services::Iappmgrservice::ChangeLifeCycleCompleteRequest
   const&) */

void pst::services::appmgrserviceClient::SizeOfChangeLifeCycleCompleteRequest
               (ChangeLifeCycleCompleteRequest *param_1)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_0002b890 + 0x2b874) - **(int **)(DAT_0002b88c + 0x2b85c);
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== WriteChangeLifeCycleCompleteRequest  @ 0002b894 ===== */

/* pst::services::appmgrserviceClient::WriteChangeLifeCycleCompleteRequest(pst::services::Iappmgrservice::ChangeLifeCycleCompleteRequest
   const&, pst::services::binder::TransactionParam&) */

void pst::services::appmgrserviceClient::WriteChangeLifeCycleCompleteRequest
               (ChangeLifeCycleCompleteRequest *param_1,TransactionParam *param_2)

{
  int iVar1;
  int iVar2;
  undefined4 *puVar3;
  undefined4 uVar4;
  
  iVar2 = **(int **)(DAT_0002b8e8 + 0x2b8a4);
  pst::services::binder::TransactionParam::PutStr((char *)param_2);
  uVar4 = *(undefined4 *)(param_1 + 0xc);
  puVar3 = (undefined4 *)pst::services::binder::TransactionParam::Alloc((uint)param_2);
  iVar1 = DAT_0002b8ec;
  if (puVar3 != (undefined4 *)0x0) {
    *puVar3 = uVar4;
  }
  if (**(int **)(iVar1 + 0x2b8d0) != iVar2) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail((*(uint *)(param_2 + 0x18) & 3) >> 1);
  }
  return;
}



/* ===== ReadStartAppRequest  @ 0002c73c ===== */

/* pst::services::appmgrserviceClient::ReadStartAppRequest(pst::services::binder::TransactionParam&,
   pst::services::Iappmgrservice::StartAppRequest&) */

void pst::services::appmgrserviceClient::ReadStartAppRequest
               (TransactionParam *param_1,StartAppRequest *param_2)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_0002c77c + 0x2c74c);
  pst::services::binder::TransactionParam::GetStr((string *)param_1);
  pst::services::binder::TransactionParam::GetStr((string *)param_1);
  if (**(int **)(DAT_0002c780 + 0x2c768) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(*(undefined4 *)(param_1 + 0x18));
  }
  return;
}



/* ===== ReadInitializeCompleteRequest  @ 0002c7c0 ===== */

/* pst::services::appmgrserviceClient::ReadInitializeCompleteRequest(pst::services::binder::TransactionParam&,
   pst::services::Iappmgrservice::InitializeCompleteRequest&) */

void pst::services::appmgrserviceClient::ReadInitializeCompleteRequest
               (TransactionParam *param_1,InitializeCompleteRequest *param_2)

{
  int iVar1;
  undefined4 *puVar2;
  undefined4 uVar3;
  
  iVar1 = **(int **)(DAT_0002c808 + 0x2c7d0);
  pst::services::binder::TransactionParam::GetStr((string *)param_1);
  puVar2 = (undefined4 *)pst::services::binder::TransactionParam::Get((uint)param_1);
  if (puVar2 == (undefined4 *)0x0) {
    uVar3 = 0;
  }
  else {
    uVar3 = *puVar2;
  }
  puVar2 = (undefined4 *)(DAT_0002c80c + 0x2c7f0);
  *(undefined4 *)(param_2 + 0xc) = uVar3;
  if (*(int *)*puVar2 != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(*(undefined4 *)(param_1 + 0x18));
  }
  return;
}



/* ===== ReadChangeLifeCycleCompleteRequest  @ 0002c810 ===== */

/* pst::services::appmgrserviceClient::ReadChangeLifeCycleCompleteRequest(pst::services::binder::TransactionParam&,
   pst::services::Iappmgrservice::ChangeLifeCycleCompleteRequest&) */

void pst::services::appmgrserviceClient::ReadChangeLifeCycleCompleteRequest
               (TransactionParam *param_1,ChangeLifeCycleCompleteRequest *param_2)

{
  int iVar1;
  undefined4 *puVar2;
  undefined4 uVar3;
  
  iVar1 = **(int **)(DAT_0002c858 + 0x2c820);
  pst::services::binder::TransactionParam::GetStr((string *)param_1);
  puVar2 = (undefined4 *)pst::services::binder::TransactionParam::Get((uint)param_1);
  if (puVar2 == (undefined4 *)0x0) {
    uVar3 = 0;
  }
  else {
    uVar3 = *puVar2;
  }
  puVar2 = (undefined4 *)(DAT_0002c85c + 0x2c840);
  *(undefined4 *)(param_2 + 0xc) = uVar3;
  if (*(int *)*puVar2 != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(*(undefined4 *)(param_1 + 0x18));
  }
  return;
}


