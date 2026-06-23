
/* ===== LifeCycleManager  @ 00013864 ===== */

void __thiscall easel::LifeCycleManager::LifeCycleManager(LifeCycleManager *this)

{
  (*(code *)PTR_LifeCycleManager_00019e58)(this);
  return;
}



/* ===== AppManagerModule  @ 00013900 ===== */

void __thiscall
easel::AppManagerModule::AppManagerModule
          (AppManagerModule *this,int param_1,char **param_2,char *param_3,
          ChangeLifeCycleHandler *param_4)

{
  (*(code *)PTR_AppManagerModule_00019e8c)(this);
  return;
}



/* ===== ~LifeCycleManager  @ 00013b04 ===== */

void __thiscall easel::LifeCycleManager::~LifeCycleManager(LifeCycleManager *this)

{
                    /* WARNING: Could not recover jumptable at 0x00013b0c. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)PTR__LifeCycleManager_00019f38)(this);
  return;
}



/* ===== ~LifeCycleManager  @ 00013b10 ===== */

void __thiscall easel::LifeCycleManager::~LifeCycleManager(LifeCycleManager *this)

{
                    /* WARNING: Could not recover jumptable at 0x00013b18. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)PTR__LifeCycleManager_00019f3c)(this);
  return;
}



/* ===== Initialize  @ 00013c54 ===== */

void pst::appmanager::AppManager::Initialize(void)

{
  (*(code *)PTR_Initialize_00019fa8)();
  return;
}



/* ===== ApplicationBase  @ 00013e38 ===== */

/* easel::ApplicationBase::ApplicationBase() */

void __thiscall easel::ApplicationBase::ApplicationBase(ApplicationBase *this)

{
  int iVar1;
  LifeCycleManager *this_00;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_00013e90 + 0x13e48);
  *(int *)this = *(int *)(DAT_00013e94 + 0x13e4a) + 8;
  *(undefined4 *)(this + 4) = 0;
  pst::core::Framework::GetReference();
  this_00 = operator_new(0x28);
  LifeCycleManager::LifeCycleManager(this_00);
  piVar2 = *(int **)(this + 4);
  *(LifeCycleManager **)(this + 4) = this_00;
  if (piVar2 != (int *)0x0) {
    (**(code **)(*piVar2 + 4))();
  }
  iVar1 = **(int **)(DAT_00013e98 + 0x13e80) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== GetAppManagerModule  @ 00013ee4 ===== */

/* easel::ApplicationBase::GetAppManagerModule() */

void __thiscall easel::ApplicationBase::GetAppManagerModule(ApplicationBase *this)

{
  undefined4 uVar1;
  int iVar2;
  
  iVar2 = **(int **)(DAT_00013f1c + 0x13ef0);
  uVar1 = LifeCycleManager::GetModuleRegistry(*(LifeCycleManager **)(this + 4));
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00013f24 + 0x13f02) == iVar2) {
                    /* WARNING: Could not recover jumptable at 0x00017774. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017778 + 0x1777c))(uVar1,*(undefined4 *)(DAT_00013f20 + 0x13f12));
    return;
  }
  __stack_chk_fail();
}



/* ===== ~ApplicationBase  @ 00013f98 ===== */

/* easel::ApplicationBase::~ApplicationBase() */

void __thiscall easel::ApplicationBase::~ApplicationBase(ApplicationBase *this)

{
  int iVar1;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_00013fe0 + 0x13fa8);
  *(int *)this = *(int *)(DAT_00013fe4 + 0x13faa) + 8;
  piVar2 = *(int **)(this + 4);
  *(undefined4 *)(this + 4) = 0;
  if (piVar2 != (int *)0x0) {
    (**(code **)(*piVar2 + 4))();
  }
  iVar1 = **(int **)(DAT_00013fe8 + 0x13fce) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== ~ApplicationBase  @ 00013fec ===== */

/* easel::ApplicationBase::~ApplicationBase() */

void __thiscall easel::ApplicationBase::~ApplicationBase(ApplicationBase *this)

{
  int iVar1;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_00014038 + 0x13ffc);
  *(int *)this = *(int *)(DAT_0001403c + 0x13ffe) + 8;
  piVar2 = *(int **)(this + 4);
  *(undefined4 *)(this + 4) = 0;
  if (piVar2 != (int *)0x0) {
    (**(code **)(*piVar2 + 4))();
  }
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00014040 + 0x14022) != iVar1) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)(DAT_00017788 + 0x1778c))(this);
  return;
}



/* ===== OnInitialize  @ 00014044 ===== */

/* easel::ApplicationBase::OnInitialize() */

void easel::ApplicationBase::OnInitialize(void)

{
  if (**(int **)(DAT_00014070 + 0x1405c) != **(int **)(DAT_0001406c + 82000)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnPostInitialize  @ 00014074 ===== */

/* easel::ApplicationBase::OnPostInitialize() */

void easel::ApplicationBase::OnPostInitialize(void)

{
  if (**(int **)(DAT_000140a0 + 0x1408c) != **(int **)(DAT_0001409c + 0x14080)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
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



/* ===== GetPostInitializeTimeout  @ 00014c8c ===== */

/* easel::ApplicationBase::GetPostInitializeTimeout() */

void easel::ApplicationBase::GetPostInitializeTimeout(void)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00014cbc + 0x14ca4) - **(int **)(DAT_00014cb8 + 0x14c98);
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1,**(int **)(DAT_00014cb8 + 0x14c98));
  }
  return;
}



/* ===== LifeCycleManager  @ 00015ce4 ===== */

/* easel::LifeCycleManager::LifeCycleManager() */

void __thiscall easel::LifeCycleManager::LifeCycleManager(LifeCycleManager *this)

{
  int iVar1;
  int iVar2;
  
  iVar2 = *(int *)(DAT_00015d24 + 0x15cf4);
  iVar1 = **(int **)(DAT_00015d20 + 0x15cf2);
  *(undefined4 *)(this + 0x20) = 0;
  *(int *)this = iVar2 + 8;
  *(undefined4 *)(this + 4) = 0;
  iVar2 = DAT_00015d28;
  *(undefined4 *)(this + 8) = 0;
  if (**(int **)(iVar2 + 0x15d0e) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== ~LifeCycleManager  @ 00015d2c ===== */

/* easel::LifeCycleManager::~LifeCycleManager() */

void __thiscall easel::LifeCycleManager::~LifeCycleManager(LifeCycleManager *this)

{
  int iVar1;
  LifeCycleManager *pLVar2;
  int *piVar3;
  code *pcVar4;
  
  iVar1 = **(int **)(DAT_00015d8c + 0x15d3c);
  *(int *)this = *(int *)(DAT_00015d90 + 0x15d3e) + 8;
  pLVar2 = *(LifeCycleManager **)(this + 0x20);
  if (pLVar2 == this + 0x10) {
    pcVar4 = *(code **)(*(int *)pLVar2 + 0x10);
  }
  else {
    if (pLVar2 == (LifeCycleManager *)0x0) goto LAB_00015d62;
    pcVar4 = *(code **)(*(int *)pLVar2 + 0x14);
  }
  (*pcVar4)();
LAB_00015d62:
  piVar3 = *(int **)(this + 8);
  *(undefined4 *)(this + 8) = 0;
  if (piVar3 != (int *)0x0) {
    (**(code **)(*piVar3 + 4))();
  }
  iVar1 = **(int **)(DAT_00015d94 + 0x15d7a) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== ~LifeCycleManager  @ 00015d98 ===== */

/* easel::LifeCycleManager::~LifeCycleManager() */

void __thiscall easel::LifeCycleManager::~LifeCycleManager(LifeCycleManager *this)

{
  int iVar1;
  LifeCycleManager *pLVar2;
  int *piVar3;
  code *pcVar4;
  
  iVar1 = **(int **)(DAT_00015dfc + 0x15da8);
  *(int *)this = *(int *)(DAT_00015e00 + 0x15daa) + 8;
  pLVar2 = *(LifeCycleManager **)(this + 0x20);
  if (pLVar2 == this + 0x10) {
    pcVar4 = *(code **)(*(int *)pLVar2 + 0x10);
  }
  else {
    if (pLVar2 == (LifeCycleManager *)0x0) goto LAB_00015dce;
    pcVar4 = *(code **)(*(int *)pLVar2 + 0x14);
  }
  (*pcVar4)();
LAB_00015dce:
  piVar3 = *(int **)(this + 8);
  *(undefined4 *)(this + 8) = 0;
  if (piVar3 != (int *)0x0) {
    (**(code **)(*piVar3 + 4))();
  }
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00015e04 + 0x15de6) != iVar1) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)(DAT_00017788 + 0x1778c))(this);
  return;
}



/* ===== ToInitialize  @ 00015f90 ===== */

/* easel::LifeCycleManager::ToInitialize() */

void __thiscall easel::LifeCycleManager::ToInitialize(LifeCycleManager *this)

{
  int iVar1;
  undefined4 *puVar2;
  undefined4 *puVar3;
  
  iVar1 = **(int **)(DAT_00015fec + 0x15fa0);
  pst::log::Print(5,DAT_00015ff8 + 0x15fac,0x5b,0,DAT_00015ff4 + 0x15faa,DAT_00015ff0 + 0x15fa6);
  puVar3 = *(undefined4 **)(*(int *)(this + 8) + 8);
  for (puVar2 = *(undefined4 **)(*(int *)(this + 8) + 4); puVar2 != puVar3; puVar2 = puVar2 + 2) {
    (**(code **)(*(int *)*puVar2 + 8))();
  }
  if (**(int **)(DAT_00015ffc + 0x15fdc) == iVar1) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== ToPostInitialize  @ 00016000 ===== */

/* easel::LifeCycleManager::ToPostInitialize() */

void __thiscall easel::LifeCycleManager::ToPostInitialize(LifeCycleManager *this)

{
  int iVar1;
  undefined4 *puVar2;
  undefined4 *puVar3;
  
  iVar1 = **(int **)(DAT_0001605c + 0x16010);
  pst::log::Print(5,DAT_00016068 + 0x1601c,0x62,0,DAT_00016064 + 0x1601a,DAT_00016060 + 0x16016);
  puVar3 = *(undefined4 **)(*(int *)(this + 8) + 8);
  for (puVar2 = *(undefined4 **)(*(int *)(this + 8) + 4); puVar2 != puVar3; puVar2 = puVar2 + 2) {
    (**(code **)(*(int *)*puVar2 + 0xc))();
  }
  if (**(int **)(DAT_0001606c + 0x1604c) == iVar1) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
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



/* ===== AppManagerModule  @ 00016d0c ===== */

/* easel::AppManagerModule::AppManagerModule(int, char**, char const*,
   pst::appmanager::ChangeLifeCycleHandler&) */

void __thiscall
easel::AppManagerModule::AppManagerModule
          (AppManagerModule *this,int param_1,char **param_2,char *param_3,
          ChangeLifeCycleHandler *param_4)

{
  AppManagerModule *pAVar1;
  int iVar2;
  int iVar3;
  
  iVar2 = **(int **)(DAT_00016d9c + 0x16d20);
  *(int *)this = *(int *)(DAT_00016da0 + 0x16d24) + 8;
  *(undefined4 *)(this + 8) = 0;
  *(undefined4 *)(this + 0xc) = 0;
  *(undefined4 *)(this + 4) = 0;
  strlen(param_3);
  std::string::__init((char *)(this + 4),(uint)param_3);
  pAVar1 = this + 0x10;
  *(undefined4 *)pAVar1 = 0;
  *(undefined4 *)(this + 0x14) = 0;
  *(undefined4 *)(this + 0x18) = 0;
  *(ChangeLifeCycleHandler **)(this + 0x1c) = param_4;
  if (0 < param_1) {
    iVar3 = 0;
    do {
      if (0 < iVar3) {
        std::string::push_back((char)pAVar1);
      }
      std::string::append((char *)pAVar1);
      iVar3 = iVar3 + 1;
    } while (param_1 != iVar3);
  }
  iVar2 = **(int **)(DAT_00016da4 + 0x16d88) - iVar2;
  if (iVar2 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar2);
  }
  return;
}



/* ===== OnInitialize  @ 00016da8 ===== */

/* easel::AppManagerModule::OnInitialize() */

void easel::AppManagerModule::OnInitialize(void)

{
  if (**(int **)(DAT_00016dd4 + 0x16dc0) != **(int **)(DAT_00016dd0 + 0x16db4)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== ~AppManagerModule  @ 00017010 ===== */

/* easel::AppManagerModule::~AppManagerModule() */

void __thiscall easel::AppManagerModule::~AppManagerModule(AppManagerModule *this)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00017058 + 0x17022);
  *(int *)this = *(int *)(DAT_0001705c + 0x17024) + 8;
  std::string::~string((string *)(this + 0x10));
  std::string::~string((string *)(this + 4));
  iVar1 = **(int **)(DAT_00017060 + 0x17048) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== ~AppManagerModule  @ 00017064 ===== */

/* easel::AppManagerModule::~AppManagerModule() */

void __thiscall easel::AppManagerModule::~AppManagerModule(AppManagerModule *this)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_000170b4 + 0x17076);
  *(int *)this = *(int *)(DAT_000170b8 + 0x17078) + 8;
  std::string::~string((string *)(this + 0x10));
  std::string::~string((string *)(this + 4));
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000170bc + 0x1709c) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))(this);
    return;
  }
  __stack_chk_fail();
}



/* ===== OnPostInitialize  @ 000170c0 ===== */

/* easel::ModuleBase<easel::AppManagerModule>::OnPostInitialize() */

void easel::ModuleBase<easel::AppManagerModule>::OnPostInitialize(void)

{
  if (**(int **)(DAT_000170ec + 0x170d8) != **(int **)(DAT_000170e8 + 0x170cc)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
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



/* ===== Initialize  @ 0001b08c ===== */

/* WARNING: Control flow encountered bad instruction data */

void pst::appmanager::AppManager::Initialize(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}


