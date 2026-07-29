
/* ===== Main  @ 0001393c ===== */

void __thiscall easel::LifeCycleManager::Main(void)

{
  (*(code *)PTR_Main_00019ea0)();
  return;
}



/* ===== OnForeground  @ 000140d4 ===== */

/* easel::ApplicationBase::OnForeground() */

void easel::ApplicationBase::OnForeground(void)

{
  if (**(int **)(DAT_00014100 + 0x140ec) != **(int **)(DAT_000140fc + 0x140e0)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== run  @ 00014574 ===== */

/* easel::ApplicationBase::run(int, char**, char const*, std::unique_ptr<easel::ModuleBaseInterface,
   std::default_delete<easel::ModuleBaseInterface> >) */

void __thiscall
easel::ApplicationBase::run
          (ApplicationBase *this,int param_1,char **param_2,char *param_3,undefined4 *param_5)

{
  int iVar1;
  int iVar2;
  int *piVar3;
  AppManagerModule *pAVar4;
  AppManager *pAVar5;
  ModuleRegistry *pMVar6;
  int iVar7;
  code *pcVar8;
  int iVar9;
  ChangeLifeCycleHandler *pCVar10;
  int iVar11;
  bool bVar12;
  ModuleRegistry *local_f4;
  int *local_f0;
  AppManagerModule *local_ec;
  int local_e8;
  ApplicationBase *pAStack_e4;
  int *local_d8;
  int local_d0;
  ApplicationBase *pAStack_cc;
  int *local_c0;
  int local_b8;
  ApplicationBase *pAStack_b4;
  int *local_a8;
  int local_a0;
  ApplicationBase *pAStack_9c;
  int *local_90;
  int local_88;
  ApplicationBase *pAStack_84;
  int *local_78;
  int local_70;
  ApplicationBase *pAStack_6c;
  int *local_60;
  int local_58;
  ApplicationBase *pAStack_54;
  int *local_48;
  int local_40;
  ApplicationBase *pAStack_3c;
  int *local_30;
  int local_28;
  
  local_28 = **(int **)(DAT_00014770 + 0x14588);
  pCVar10 = *(ChangeLifeCycleHandler **)(this + 4);
  pAVar4 = operator_new(0x20);
  AppManagerModule::AppManagerModule(pAVar4,param_1,param_2,param_3,pCVar10);
  iVar2 = DAT_00014784;
  iVar11 = DAT_00014780;
  iVar1 = DAT_0001477c;
  iVar7 = DAT_00014778;
  iVar9 = DAT_00014774 + 0x145bc;
  pAVar5 = (AppManager *)AppManagerModule::GetAppManager();
  local_58 = iVar2 + 0x145cc;
  local_70 = iVar11 + 0x145c6;
  local_88 = iVar7 + 0x145c0;
  local_a0 = iVar1 + 0x145c4;
  pAStack_9c = this;
  local_90 = &local_a0;
  pAStack_84 = this;
  local_78 = &local_88;
  pAStack_6c = this;
  local_60 = &local_70;
  pAStack_54 = this;
  local_48 = &local_58;
  local_40 = iVar9;
  pAStack_3c = this;
  local_30 = &local_40;
  pst::appmanager::AppManager::SetPowerStateHandler
            (pAVar5,&local_40,&local_58,&local_70,&local_88,&local_a0);
  if (local_90 == &local_a0) {
    pcVar8 = *(code **)(*local_90 + 0x10);
LAB_00014624:
    (*pcVar8)();
  }
  else if (local_90 != (int *)0x0) {
    pcVar8 = *(code **)(*local_90 + 0x14);
    goto LAB_00014624;
  }
  if (local_78 == &local_88) {
    pcVar8 = *(code **)(*local_78 + 0x10);
LAB_00014638:
    (*pcVar8)();
  }
  else if (local_78 != (int *)0x0) {
    pcVar8 = *(code **)(*local_78 + 0x14);
    goto LAB_00014638;
  }
  if (local_60 == &local_70) {
    pcVar8 = *(code **)(*local_60 + 0x10);
LAB_0001464c:
    (*pcVar8)();
  }
  else if (local_60 != (int *)0x0) {
    pcVar8 = *(code **)(*local_60 + 0x14);
    goto LAB_0001464c;
  }
  if (local_48 == &local_58) {
    pcVar8 = *(code **)(*local_48 + 0x10);
LAB_00014660:
    (*pcVar8)();
  }
  else if (local_48 != (int *)0x0) {
    pcVar8 = *(code **)(*local_48 + 0x14);
    goto LAB_00014660;
  }
  if (local_30 == &local_40) {
    pcVar8 = *(code **)(*local_30 + 0x10);
LAB_00014674:
    (*pcVar8)();
  }
  else if (local_30 != (int *)0x0) {
    pcVar8 = *(code **)(*local_30 + 0x14);
    goto LAB_00014674;
  }
  iVar1 = DAT_00014790;
  iVar7 = DAT_0001478c;
  iVar11 = DAT_00014788 + 0x14688;
  pAVar5 = (AppManager *)AppManagerModule::GetAppManager();
  local_d0 = iVar1 + 0x14690;
  local_e8 = iVar7 + 0x1468a;
  pAStack_e4 = this;
  local_d8 = &local_e8;
  pAStack_cc = this;
  local_c0 = &local_d0;
  local_b8 = iVar11;
  pAStack_b4 = this;
  local_a8 = &local_b8;
  pst::appmanager::AppManager::SetResetSettingHandler(pAVar5,&local_b8,&local_d0,&local_e8);
  if (local_d8 == &local_e8) {
    pcVar8 = *(code **)(*local_d8 + 0x10);
LAB_000146c6:
    (*pcVar8)();
  }
  else if (local_d8 != (int *)0x0) {
    pcVar8 = *(code **)(*local_d8 + 0x14);
    goto LAB_000146c6;
  }
  if (local_c0 == &local_d0) {
    pcVar8 = *(code **)(*local_c0 + 0x10);
LAB_000146da:
    (*pcVar8)();
  }
  else if (local_c0 != (int *)0x0) {
    pcVar8 = *(code **)(*local_c0 + 0x14);
    goto LAB_000146da;
  }
  if (local_a8 == &local_b8) {
    pcVar8 = *(code **)(*local_a8 + 0x10);
  }
  else {
    if (local_a8 == (int *)0x0) goto LAB_000146f0;
    pcVar8 = *(code **)(*local_a8 + 0x14);
  }
  (*pcVar8)();
LAB_000146f0:
  pMVar6 = operator_new(0x10);
  ModuleRegistry::ModuleRegistry(pMVar6);
  local_ec = pAVar4;
  ModuleRegistry::Register(pMVar6,&local_ec);
  pAVar4 = local_ec;
  local_ec = (AppManagerModule *)0x0;
  if (pAVar4 != (AppManagerModule *)0x0) {
    (**(code **)(*(int *)pAVar4 + 4))();
  }
  local_f0 = (int *)*param_5;
  *param_5 = 0;
  ModuleRegistry::Register(pMVar6,&local_f0);
  piVar3 = local_f0;
  local_f0 = (int *)0x0;
  if (piVar3 != (int *)0x0) {
    (**(code **)(*piVar3 + 4))();
  }
  local_f4 = pMVar6;
  LifeCycleManager::Main(*(LifeCycleManager **)(this + 4),&local_f4);
  pMVar6 = local_f4;
  bVar12 = local_f4 != (ModuleRegistry *)0x0;
  local_f4 = (ModuleRegistry *)0x0;
  if (bVar12) {
    (**(code **)(*(int *)pMVar6 + 4))();
  }
  iVar7 = **(int **)(DAT_00014794 + 0x1475c) - local_28;
  if (iVar7 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar7);
  }
  return;
}



/* ===== Main  @ 00015e08 ===== */

/* easel::LifeCycleManager::Main(std::unique_ptr<easel::ModuleRegistry,
   std::default_delete<easel::ModuleRegistry> >) */

void __thiscall easel::LifeCycleManager::Main(LifeCycleManager *this,undefined4 *param_2)

{
  int *piVar1;
  LifeCycleManager *pLVar2;
  code *pcVar3;
  undefined4 uVar4;
  int iVar5;
  undefined4 *puVar6;
  undefined4 *puVar7;
  Framework aFStack_34 [4];
  int aiStack_30 [4];
  int *local_20;
  int local_18;
  
  local_18 = **(int **)(DAT_00015f58 + 0x15e16);
  uVar4 = *param_2;
  *param_2 = 0;
  piVar1 = *(int **)(this + 8);
  *(undefined4 *)(this + 8) = uVar4;
  if (piVar1 != (int *)0x0) {
    (**(code **)(*piVar1 + 4))();
  }
  pst::log::Print(5,DAT_00015f5c + 0x15e3c,0x5b,0,DAT_00015f60 + 0x15e3e,DAT_00015f64 + 0x15e3a);
  puVar7 = *(undefined4 **)(*(int *)(this + 8) + 8);
  for (puVar6 = *(undefined4 **)(*(int *)(this + 8) + 4); puVar6 != puVar7; puVar6 = puVar6 + 2) {
    (**(code **)(*(int *)*puVar6 + 8))();
  }
  pLVar2 = *(LifeCycleManager **)(this + 0x20);
  if (pLVar2 == (LifeCycleManager *)0x0) {
    local_20 = (int *)0x0;
  }
  else if (pLVar2 == this + 0x10) {
    local_20 = aiStack_30;
    (**(code **)(*(int *)pLVar2 + 0xc))();
  }
  else {
    local_20 = (int *)(**(code **)(*(int *)pLVar2 + 8))();
  }
  Framework::Framework(aFStack_34,aiStack_30);
  if (local_20 == aiStack_30) {
    pcVar3 = *(code **)(*local_20 + 0x10);
  }
  else {
    if (local_20 == (int *)0x0) goto LAB_00015ea2;
    pcVar3 = *(code **)(*local_20 + 0x14);
  }
  (*pcVar3)();
LAB_00015ea2:
  pst::log::Print(5,DAT_00015f68 + 0x15eb0,0x62,0,DAT_00015f6c + 0x15eb2,DAT_00015f70 + 0x15eae);
  puVar7 = *(undefined4 **)(*(int *)(this + 8) + 8);
  for (puVar6 = *(undefined4 **)(*(int *)(this + 8) + 4); puVar6 != puVar7; puVar6 = puVar6 + 2) {
    (**(code **)(*(int *)*puVar6 + 0xc))();
  }
  pst::log::Print(5,DAT_00015f74 + 0x15ee2,0x69,0,DAT_00015f78 + 0x15ee4,DAT_00015f7c + 0x15ee0);
  puVar7 = *(undefined4 **)(*(int *)(this + 8) + 8);
  for (puVar6 = *(undefined4 **)(*(int *)(this + 8) + 4); puVar6 != puVar7; puVar6 = puVar6 + 2) {
    (**(code **)(*(int *)*puVar6 + 0x10))();
  }
  Framework::~Framework(aFStack_34);
  pst::log::Print(5,DAT_00015f84 + 0x15f1c,0x70,0,DAT_00015f80 + 0x15f1a,DAT_00015f88 + 0x15f18);
  iVar5 = *(int *)(this + 8);
  puVar6 = *(undefined4 **)(iVar5 + 8);
  puVar7 = *(undefined4 **)(iVar5 + 4);
  while (puVar6 != puVar7) {
    puVar6 = puVar6 + -2;
    (**(code **)(*(int *)*puVar6 + 0x20))();
    puVar7 = *(undefined4 **)(iVar5 + 4);
  }
  iVar5 = **(int **)(DAT_00015f8c + 0x15f46) - local_18;
  if (iVar5 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar5);
  }
  return;
}



/* ===== ToActivate  @ 00016070 ===== */

/* easel::LifeCycleManager::ToActivate() */

void __thiscall easel::LifeCycleManager::ToActivate(LifeCycleManager *this)

{
  int iVar1;
  undefined4 *puVar2;
  undefined4 *puVar3;
  
  iVar1 = **(int **)(DAT_000160cc + 0x16080);
  pst::log::Print(5,DAT_000160d8 + 0x1608c,0x69,0,DAT_000160d4 + 0x1608a,DAT_000160d0 + 0x16086);
  puVar3 = *(undefined4 **)(*(int *)(this + 8) + 8);
  for (puVar2 = *(undefined4 **)(*(int *)(this + 8) + 4); puVar2 != puVar3; puVar2 = puVar2 + 2) {
    (**(code **)(*(int *)*puVar2 + 0x10))();
  }
  if (**(int **)(DAT_000160dc + 0x160bc) == iVar1) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== OnForeground  @ 00016400 ===== */

/* easel::LifeCycleManager::OnForeground() */

void __thiscall easel::LifeCycleManager::OnForeground(LifeCycleManager *this)

{
  int iVar1;
  undefined4 *puVar2;
  int iVar3;
  undefined4 *puVar4;
  
  iVar1 = **(int **)(DAT_00016460 + 0x16410);
  pst::log::Print(5,DAT_0001646c + 0x1641c,0x45,0,DAT_00016468 + 0x1641a,DAT_00016464 + 0x16416);
  iVar3 = *(int *)(this + 8);
  puVar4 = *(undefined4 **)(iVar3 + 8);
  puVar2 = *(undefined4 **)(iVar3 + 4);
  while (puVar4 != puVar2) {
    puVar4 = puVar4 + -2;
    (**(code **)(*(int *)*puVar4 + 0x14))();
    puVar2 = *(undefined4 **)(iVar3 + 4);
  }
  *(undefined4 *)(this + 4) = 1;
  if (**(int **)(DAT_00016470 + 0x16450) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== Run  @ 000167e4 ===== */

/* pst::core::FuncJob::Run() */

void __thiscall pst::core::FuncJob::Run(FuncJob *this)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00016818 + 0x167f4) != **(int **)(DAT_00016814 + 0x167f2)) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x00016812. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (**(code **)(**(int **)(this + 0x28) + 0x18))();
  return;
}



/* ===== OnForeground  @ 000170f0 ===== */

/* easel::ModuleBase<easel::AppManagerModule>::OnForeground() */

void easel::ModuleBase<easel::AppManagerModule>::OnForeground(void)

{
  if (**(int **)(DAT_0001711c + 0x17108) != **(int **)(DAT_00017118 + 0x170fc)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}


