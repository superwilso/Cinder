
/* ===== AppModuleBase  @ 00011af0 ===== */

void __thiscall easel::AppModuleBase<easel::CuiAppModule>::AppModuleBase(void)

{
  (*(code *)PTR_AppModuleBase_00014f24)();
  return;
}



/* ===== ~AppModuleBase  @ 00011bec ===== */

void __thiscall
easel::AppModuleBase<easel::CuiAppModule>::~AppModuleBase(AppModuleBase<easel::CuiAppModule> *this)

{
  (*(code *)PTR__AppModuleBase_00014f78)(this);
  return;
}



/* ===== ~CuiAppModule  @ 00011c1c ===== */

void __thiscall easel::CuiAppModule::~CuiAppModule(CuiAppModule *this)

{
                    /* WARNING: Could not recover jumptable at 0x00011c24. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)PTR__CuiAppModule_00014f88)(this);
  return;
}



/* ===== CuiAppModule  @ 00011e60 ===== */

/* easel::CuiAppModule::CuiAppModule(easel::ApplicationBase&, int, char**, std::function<void ()>,
   std::function<void ()>, std::function<void ()>, std::function<void ()>, std::function<void ()>,
   std::function<bool ()>, std::function<void ()>) */

void __thiscall
easel::CuiAppModule::CuiAppModule
          (CuiAppModule *this,undefined4 param_1,undefined4 param_2,undefined4 param_3,int *param_5,
          int *param_6,int *param_7,int *param_8,int *param_9,int *param_10,int *param_11)

{
  undefined4 uVar1;
  undefined4 uVar2;
  undefined4 uVar3;
  int *piVar4;
  int iVar5;
  code *pcVar6;
  int aiStack_d0 [4];
  int *local_c0;
  int aiStack_b8 [4];
  int *local_a8;
  int aiStack_a0 [4];
  int *local_90;
  int aiStack_88 [4];
  int *local_78;
  int aiStack_70 [4];
  int *local_60;
  int aiStack_58 [4];
  int *local_48;
  int aiStack_40 [4];
  int *local_30;
  int local_28;
  
  local_28 = **(int **)(DAT_00012078 + 0x11e76);
  piVar4 = (int *)param_5[4];
  if (piVar4 == (int *)0x0) {
    local_30 = (int *)0x0;
  }
  else if (piVar4 == param_5) {
    local_30 = aiStack_40;
    (**(code **)(*piVar4 + 0xc))();
  }
  else {
    local_30 = (int *)(**(code **)(*piVar4 + 8))();
  }
  piVar4 = (int *)param_6[4];
  if (piVar4 == (int *)0x0) {
    local_48 = (int *)0x0;
  }
  else if (piVar4 == param_6) {
    local_48 = aiStack_58;
    (**(code **)(*piVar4 + 0xc))();
  }
  else {
    local_48 = (int *)(**(code **)(*piVar4 + 8))();
  }
  piVar4 = (int *)param_7[4];
  if (piVar4 == (int *)0x0) {
    local_60 = (int *)0x0;
  }
  else if (piVar4 == param_7) {
    local_60 = aiStack_70;
    (**(code **)(*piVar4 + 0xc))();
  }
  else {
    local_60 = (int *)(**(code **)(*piVar4 + 8))();
  }
  piVar4 = (int *)param_8[4];
  if (piVar4 == (int *)0x0) {
    local_78 = (int *)0x0;
  }
  else if (piVar4 == param_8) {
    local_78 = aiStack_88;
    (**(code **)(*piVar4 + 0xc))();
  }
  else {
    local_78 = (int *)(**(code **)(*piVar4 + 8))();
  }
  piVar4 = (int *)param_9[4];
  if (piVar4 == (int *)0x0) {
    local_90 = (int *)0x0;
  }
  else if (piVar4 == param_9) {
    local_90 = aiStack_a0;
    (**(code **)(*piVar4 + 0xc))();
  }
  else {
    local_90 = (int *)(**(code **)(*piVar4 + 8))();
  }
  piVar4 = (int *)param_10[4];
  if (piVar4 == (int *)0x0) {
    local_a8 = (int *)0x0;
  }
  else if (piVar4 == param_10) {
    local_a8 = aiStack_b8;
    (**(code **)(*piVar4 + 0xc))();
  }
  else {
    local_a8 = (int *)(**(code **)(*piVar4 + 8))();
  }
  piVar4 = (int *)param_11[4];
  if (piVar4 == (int *)0x0) {
    local_c0 = (int *)0x0;
  }
  else if (piVar4 == param_11) {
    local_c0 = aiStack_d0;
    (**(code **)(*piVar4 + 0xc))();
  }
  else {
    local_c0 = (int *)(**(code **)(*piVar4 + 8))();
  }
  AppModuleBase<easel::CuiAppModule>::AppModuleBase
            ((AppModuleBase<easel::CuiAppModule> *)this,param_1,param_2,param_3,aiStack_40,
             aiStack_58,aiStack_70,aiStack_88,aiStack_a0,aiStack_b8,aiStack_d0);
  if (local_c0 == aiStack_d0) {
    pcVar6 = *(code **)(*local_c0 + 0x10);
LAB_00011fa0:
    (*pcVar6)();
  }
  else if (local_c0 != (int *)0x0) {
    pcVar6 = *(code **)(*local_c0 + 0x14);
    goto LAB_00011fa0;
  }
  if (local_a8 == aiStack_b8) {
    pcVar6 = *(code **)(*local_a8 + 0x10);
LAB_00011fb4:
    (*pcVar6)();
  }
  else if (local_a8 != (int *)0x0) {
    pcVar6 = *(code **)(*local_a8 + 0x14);
    goto LAB_00011fb4;
  }
  if (local_90 == aiStack_a0) {
    pcVar6 = *(code **)(*local_90 + 0x10);
LAB_00011fc8:
    (*pcVar6)();
  }
  else if (local_90 != (int *)0x0) {
    pcVar6 = *(code **)(*local_90 + 0x14);
    goto LAB_00011fc8;
  }
  if (local_78 == aiStack_88) {
    pcVar6 = *(code **)(*local_78 + 0x10);
LAB_00011fdc:
    (*pcVar6)();
  }
  else if (local_78 != (int *)0x0) {
    pcVar6 = *(code **)(*local_78 + 0x14);
    goto LAB_00011fdc;
  }
  if (local_60 == aiStack_70) {
    pcVar6 = *(code **)(*local_60 + 0x10);
LAB_00011ff0:
    (*pcVar6)();
  }
  else if (local_60 != (int *)0x0) {
    pcVar6 = *(code **)(*local_60 + 0x14);
    goto LAB_00011ff0;
  }
  if (local_48 == aiStack_58) {
    pcVar6 = *(code **)(*local_48 + 0x10);
LAB_00012004:
    (*pcVar6)();
  }
  else if (local_48 != (int *)0x0) {
    pcVar6 = *(code **)(*local_48 + 0x14);
    goto LAB_00012004;
  }
  if (local_30 == aiStack_40) {
    pcVar6 = *(code **)(*local_30 + 0x10);
  }
  else {
    if (local_30 == (int *)0x0) goto LAB_0001201c;
    pcVar6 = *(code **)(*local_30 + 0x14);
  }
  (*pcVar6)();
LAB_0001201c:
  uVar1 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x4);
  uVar2 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0x8);
  uVar3 = *(undefined4 *)((undefined1  [16])0x0 + (undefined1  [16])0xc);
  *(int *)this = *(int *)(DAT_0001207c + 0x12026) + 8;
  *(undefined4 *)(this + 0xe9) = 0;
  *(undefined4 *)(this + 0xed) = uVar1;
  *(undefined4 *)(this + 0xf1) = uVar2;
  *(undefined4 *)(this + 0xf5) = uVar3;
  *(undefined4 *)(this + 0xe0) = 0;
  *(undefined4 *)(this + 0xe4) = uVar1;
  *(undefined4 *)(this + 0xe8) = uVar2;
  *(undefined4 *)(this + 0xec) = uVar3;
  *(undefined4 *)(this + 0xd0) = 0;
  *(undefined4 *)(this + 0xd4) = uVar1;
  *(undefined4 *)(this + 0xd8) = uVar2;
  *(undefined4 *)(this + 0xdc) = uVar3;
  *(undefined4 *)(this + 0xc0) = 0;
  *(undefined4 *)(this + 0xc4) = uVar1;
  *(undefined4 *)(this + 200) = uVar2;
  *(undefined4 *)(this + 0xcc) = uVar3;
  *(undefined4 *)(this + 0xb0) = 0;
  *(undefined4 *)(this + 0xb4) = uVar1;
  *(undefined4 *)(this + 0xb8) = uVar2;
  *(undefined4 *)(this + 0xbc) = uVar3;
  this[0xf9] = (CuiAppModule)0x1;
  this[0xfa] = (CuiAppModule)0x1;
  iVar5 = **(int **)(DAT_00012080 + 0x12064) - local_28;
  if (iVar5 == 0) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar5);
}



/* ===== AppModuleBase  @ 00012084 ===== */

/* easel::AppModuleBase<easel::CuiAppModule>::AppModuleBase(easel::ApplicationBase&, int, char**,
   std::function<void ()>, std::function<void ()>, std::function<void ()>, std::function<void ()>,
   std::function<void ()>, std::function<bool ()>, std::function<void ()>) */

void __thiscall
easel::AppModuleBase<easel::CuiAppModule>::AppModuleBase
          (AppModuleBase<easel::CuiAppModule> *this,undefined4 param_1,undefined4 param_3,
          undefined4 param_4,int *param_5,int *param_6,int *param_7,int *param_8,int *param_9,
          int *param_10,int *param_11)

{
  int iVar1;
  int *piVar2;
  undefined4 uVar3;
  
  iVar1 = **(int **)(DAT_000121d0 + 0x12094);
  *(int *)this = *(int *)(DAT_000121d4 + 0x12096) + 8;
  *(undefined4 *)(this + 4) = param_1;
  piVar2 = (int *)param_5[4];
  if (piVar2 == (int *)0x0) {
    uVar3 = 0;
LAB_000120ba:
    *(undefined4 *)(this + 0x18) = uVar3;
  }
  else {
    if (piVar2 != param_5) {
      uVar3 = (**(code **)(*piVar2 + 8))();
      goto LAB_000120ba;
    }
    *(AppModuleBase<easel::CuiAppModule> **)(this + 0x18) = this + 8;
    (**(code **)(*(int *)param_5[4] + 0xc))();
  }
  piVar2 = (int *)param_6[4];
  if (piVar2 == (int *)0x0) {
    uVar3 = 0;
LAB_000120e0:
    *(undefined4 *)(this + 0x30) = uVar3;
  }
  else {
    if (piVar2 != param_6) {
      uVar3 = (**(code **)(*piVar2 + 8))();
      goto LAB_000120e0;
    }
    *(AppModuleBase<easel::CuiAppModule> **)(this + 0x30) = this + 0x20;
    (**(code **)(*(int *)param_6[4] + 0xc))();
  }
  piVar2 = (int *)param_7[4];
  if (piVar2 == (int *)0x0) {
    uVar3 = 0;
LAB_00012106:
    *(undefined4 *)(this + 0x48) = uVar3;
  }
  else {
    if (piVar2 != param_7) {
      uVar3 = (**(code **)(*piVar2 + 8))();
      goto LAB_00012106;
    }
    *(AppModuleBase<easel::CuiAppModule> **)(this + 0x48) = this + 0x38;
    (**(code **)(*(int *)param_7[4] + 0xc))();
  }
  piVar2 = (int *)param_8[4];
  if (piVar2 == (int *)0x0) {
    uVar3 = 0;
LAB_0001212c:
    *(undefined4 *)(this + 0x60) = uVar3;
  }
  else {
    if (piVar2 != param_8) {
      uVar3 = (**(code **)(*piVar2 + 8))();
      goto LAB_0001212c;
    }
    *(AppModuleBase<easel::CuiAppModule> **)(this + 0x60) = this + 0x50;
    (**(code **)(*(int *)param_8[4] + 0xc))();
  }
  piVar2 = (int *)param_9[4];
  if (piVar2 == (int *)0x0) {
    uVar3 = 0;
LAB_00012152:
    *(undefined4 *)(this + 0x78) = uVar3;
  }
  else {
    if (piVar2 != param_9) {
      uVar3 = (**(code **)(*piVar2 + 8))();
      goto LAB_00012152;
    }
    *(AppModuleBase<easel::CuiAppModule> **)(this + 0x78) = this + 0x68;
    (**(code **)(*(int *)param_9[4] + 0xc))();
  }
  piVar2 = (int *)param_10[4];
  if (piVar2 == (int *)0x0) {
    uVar3 = 0;
LAB_00012178:
    *(undefined4 *)(this + 0x90) = uVar3;
  }
  else {
    if (piVar2 != param_10) {
      uVar3 = (**(code **)(*piVar2 + 8))();
      goto LAB_00012178;
    }
    *(AppModuleBase<easel::CuiAppModule> **)(this + 0x90) = this + 0x80;
    (**(code **)(*(int *)param_10[4] + 0xc))();
  }
  piVar2 = (int *)param_11[4];
  if (piVar2 == (int *)0x0) {
    uVar3 = 0;
  }
  else {
    if (piVar2 == param_11) {
      *(AppModuleBase<easel::CuiAppModule> **)(this + 0xa8) = this + 0x98;
      (**(code **)(*(int *)param_11[4] + 0xc))();
      goto LAB_000121b6;
    }
    uVar3 = (**(code **)(*piVar2 + 8))();
  }
  *(undefined4 *)(this + 0xa8) = uVar3;
LAB_000121b6:
  iVar1 = **(int **)(DAT_000121d8 + 0x121be) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== OnInitialize  @ 000121dc ===== */

/* easel::CuiAppModule::OnInitialize() */

void __thiscall easel::CuiAppModule::OnInitialize(CuiAppModule *this)

{
  code *pcVar1;
  ulonglong uVar2;
  undefined4 local_50;
  undefined4 uStack_4c;
  undefined4 local_48;
  TimeMeasureHolder aTStack_44 [20];
  int local_30;
  CuiAppModule *pCStack_2c;
  int *local_20;
  int local_14;
  
  local_14 = **(int **)(DAT_00012270 + 0x121ea);
  uVar2 = pst::core::Framework::GetReference();
  pst::core::Framework::UpdateCurrentContextHangTimeout(uVar2);
  local_50 = 0;
  uStack_4c = 0;
  local_48 = 0;
  std::string::__init((char *)&local_50,DAT_00012274 + 0x1220c);
  pst::services::binder::ServiceManager::TimeMeasureHolder::TimeMeasureHolder
            (aTStack_44,(string *)&local_50,false);
  std::string::~string((string *)&local_50);
  (**(code **)(**(int **)(this + 0x18) + 0x18))();
  local_30 = DAT_00012278 + 0x12238;
  pCStack_2c = this;
  local_20 = &local_30;
  easel::ApplicationBase::SetPumpTriggerHandler(*(undefined4 *)(this + 4),&local_30);
  if (local_20 == &local_30) {
    pcVar1 = *(code **)(*local_20 + 0x10);
  }
  else {
    if (local_20 == (int *)0x0) goto LAB_00012252;
    pcVar1 = *(code **)(*local_20 + 0x14);
  }
  (*pcVar1)();
LAB_00012252:
  pst::services::binder::ServiceManager::TimeMeasureHolder::~TimeMeasureHolder(aTStack_44);
  if (**(int **)(DAT_0001227c + 0x12260) != local_14) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnInitialize  @ 00012280 ===== */

/* easel::AppModuleBase<easel::CuiAppModule>::OnInitialize() */

void __thiscall
easel::AppModuleBase<easel::CuiAppModule>::OnInitialize(AppModuleBase<easel::CuiAppModule> *this)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000122b4 + 0x12290) != **(int **)(DAT_000122b0 + 0x1228e)) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x000122ae. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (**(code **)(**(int **)(this + 0x18) + 0x18))();
  return;
}



/* ===== OnForeground  @ 00012548 ===== */

/* easel::CuiAppModule::OnForeground() */

void __thiscall easel::CuiAppModule::OnForeground(CuiAppModule *this)

{
  undefined4 uVar1;
  undefined4 extraout_r1;
  undefined4 extraout_r1_00;
  undefined4 local_50;
  undefined4 uStack_4c;
  undefined4 local_48;
  undefined4 local_40;
  undefined4 uStack_3c;
  undefined4 local_38;
  TimeMeasureHolder aTStack_30 [20];
  int local_1c;
  
  local_1c = **(int **)(DAT_00012614 + 0x12558);
  if (this[0xfa] != (CuiAppModule)0x0) {
    uVar1 = pst::core::Framework::GetReference();
    easel::ApplicationBase::GetChangeLifeCycleTimeout();
    pst::core::Framework::UpdateCurrentContextHangTimeout(CONCAT44(extraout_r1,uVar1));
    local_40 = 0;
    uStack_3c = 0;
    local_38 = 0;
    std::string::__init((char *)&local_40,DAT_00012618 + 0x12582);
    pst::services::binder::ServiceManager::TimeMeasureHolder::TimeMeasureHolder
              (aTStack_30,(string *)&local_40,false);
    std::string::~string((string *)&local_40);
    (**(code **)(**(int **)(this + 0x48) + 0x18))();
    this[0xfa] = (CuiAppModule)0x0;
    pst::services::binder::ServiceManager::TimeMeasureHolder::~TimeMeasureHolder(aTStack_30);
  }
  uVar1 = pst::core::Framework::GetReference();
  easel::ApplicationBase::GetChangeLifeCycleTimeout();
  pst::core::Framework::UpdateCurrentContextHangTimeout(CONCAT44(extraout_r1_00,uVar1));
  local_50 = 0;
  uStack_4c = 0;
  local_48 = 0;
  std::string::__init((char *)&local_50,DAT_0001261c + 0x125d4);
  pst::services::binder::ServiceManager::TimeMeasureHolder::TimeMeasureHolder
            (aTStack_30,(string *)&local_50,false);
  std::string::~string((string *)&local_50);
  (**(code **)(**(int **)(this + 0x60) + 0x18))();
  pst::services::binder::ServiceManager::TimeMeasureHolder::~TimeMeasureHolder(aTStack_30);
  if (**(int **)(DAT_00012620 + 0x12604) != local_1c) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnForeground  @ 0001265c ===== */

/* easel::AppModuleBase<easel::CuiAppModule>::OnForeground() */

void __thiscall
easel::AppModuleBase<easel::CuiAppModule>::OnForeground(AppModuleBase<easel::CuiAppModule> *this)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00012690 + 0x1266c) != **(int **)(DAT_0001268c + 0x1266a)) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x0001268a. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (**(code **)(**(int **)(this + 0x60) + 0x18))();
  return;
}



/* ===== OnPumpTrigger  @ 000128c0 ===== */

/* easel::CuiAppModule::OnPumpTrigger() */

void __thiscall easel::CuiAppModule::OnPumpTrigger(CuiAppModule *this)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00012908 + 0x128d2);
  std::mutex::lock();
  this[0xf8] = (CuiAppModule)0x1;
  std::condition_variable::notify_one();
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_0001290c + 0x128f2) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x00013234. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00013238 + 0x1323c))(this + 0xb0);
    return;
  }
  __stack_chk_fail();
}



/* ===== ~CuiAppModule  @ 00012910 ===== */

/* easel::CuiAppModule::~CuiAppModule() */

void __thiscall easel::CuiAppModule::~CuiAppModule(CuiAppModule *this)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00012960 + 0x12922);
  *(int *)this = *(int *)(DAT_00012964 + 0x12924) + 8;
  std::condition_variable::~condition_variable((condition_variable *)(this + 200));
  std::mutex::~mutex((mutex *)(this + 0xb0));
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00012968 + 0x12948) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x00013244. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00013248 + 0x1324c))(this);
    return;
  }
  __stack_chk_fail();
}



/* ===== ~CuiAppModule  @ 0001296c ===== */

/* easel::CuiAppModule::~CuiAppModule() */

void __thiscall easel::CuiAppModule::~CuiAppModule(CuiAppModule *this)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_000129c0 + 0x1297e);
  *(int *)this = *(int *)(DAT_000129c4 + 0x12980) + 8;
  std::condition_variable::~condition_variable((condition_variable *)(this + 200));
  std::mutex::~mutex((mutex *)(this + 0xb0));
  AppModuleBase<easel::CuiAppModule>::~AppModuleBase((AppModuleBase<easel::CuiAppModule> *)this);
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000129c8 + 0x129aa) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x00013254. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(_fini + DAT_00013258))(this);
    return;
  }
  __stack_chk_fail();
}



/* ===== ~AppModuleBase  @ 00012da4 ===== */

/* easel::AppModuleBase<easel::CuiAppModule>::~AppModuleBase() */

void __thiscall
easel::AppModuleBase<easel::CuiAppModule>::~AppModuleBase(AppModuleBase<easel::CuiAppModule> *this)

{
  int iVar1;
  AppModuleBase<easel::CuiAppModule> *pAVar2;
  code *pcVar3;
  
  iVar1 = **(int **)(DAT_00012e88 + 0x12db4);
  *(int *)this = *(int *)(DAT_00012e8c + 0x12db6) + 8;
  pAVar2 = *(AppModuleBase<easel::CuiAppModule> **)(this + 0xa8);
  if (pAVar2 == this + 0x98) {
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x10);
LAB_00012dda:
    (*pcVar3)();
  }
  else if (pAVar2 != (AppModuleBase<easel::CuiAppModule> *)0x0) {
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x14);
    goto LAB_00012dda;
  }
  pAVar2 = *(AppModuleBase<easel::CuiAppModule> **)(this + 0x90);
  if (pAVar2 == this + 0x80) {
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x10);
LAB_00012df4:
    (*pcVar3)();
  }
  else if (pAVar2 != (AppModuleBase<easel::CuiAppModule> *)0x0) {
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x14);
    goto LAB_00012df4;
  }
  pAVar2 = *(AppModuleBase<easel::CuiAppModule> **)(this + 0x78);
  if (pAVar2 == this + 0x68) {
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x10);
LAB_00012e0c:
    (*pcVar3)();
  }
  else if (pAVar2 != (AppModuleBase<easel::CuiAppModule> *)0x0) {
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x14);
    goto LAB_00012e0c;
  }
  pAVar2 = *(AppModuleBase<easel::CuiAppModule> **)(this + 0x60);
  if (pAVar2 == this + 0x50) {
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x10);
LAB_00012e24:
    (*pcVar3)();
  }
  else if (pAVar2 != (AppModuleBase<easel::CuiAppModule> *)0x0) {
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x14);
    goto LAB_00012e24;
  }
  pAVar2 = *(AppModuleBase<easel::CuiAppModule> **)(this + 0x48);
  if (pAVar2 == this + 0x38) {
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x10);
LAB_00012e3c:
    (*pcVar3)();
  }
  else if (pAVar2 != (AppModuleBase<easel::CuiAppModule> *)0x0) {
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x14);
    goto LAB_00012e3c;
  }
  pAVar2 = *(AppModuleBase<easel::CuiAppModule> **)(this + 0x30);
  if (pAVar2 == this + 0x20) {
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x10);
LAB_00012e54:
    (*pcVar3)();
  }
  else if (pAVar2 != (AppModuleBase<easel::CuiAppModule> *)0x0) {
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x14);
    goto LAB_00012e54;
  }
  pAVar2 = *(AppModuleBase<easel::CuiAppModule> **)(this + 0x18);
  if (pAVar2 == this + 8) {
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x10);
  }
  else {
    if (pAVar2 == (AppModuleBase<easel::CuiAppModule> *)0x0) goto LAB_00012e6e;
    pcVar3 = *(code **)(*(int *)pAVar2 + 0x14);
  }
  (*pcVar3)();
LAB_00012e6e:
  iVar1 = **(int **)(DAT_00012e90 + 0x12e76) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== ~AppModuleBase  @ 00012e94 ===== */

/* easel::AppModuleBase<easel::CuiAppModule>::~AppModuleBase() */

void __thiscall
easel::AppModuleBase<easel::CuiAppModule>::~AppModuleBase(AppModuleBase<easel::CuiAppModule> *this)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00012ec4 + 0x12ea0);
  ~AppModuleBase(this);
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00012ec8 + 0x12eb0) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x00013254. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(_fini + DAT_00013258))();
    return;
  }
  __stack_chk_fail();
}


