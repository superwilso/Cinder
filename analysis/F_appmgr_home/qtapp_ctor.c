
/* ===== QtApplication  @ 00010d60 ===== */

/* easel::QtApplication::QtApplication(int&, char**) */

void __thiscall easel::QtApplication::QtApplication(QtApplication *this,int *param_1,char **param_2)

{
  int *piVar1;
  int iVar2;
  
  iVar2 = **(int **)(DAT_00010d9c + 0x10d6c);
  piVar1 = (int *)QApplication::QApplication((QApplication *)this,param_1,param_2,0x50302);
  *piVar1 = *(int *)(DAT_00010da0 + 0x10d82) + 8;
  if (**(int **)(DAT_00010da4 + 0x10d8e) != iVar2) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== ~QtApplication  @ 00010df4 ===== */

/* easel::QtApplication::~QtApplication() */

void __thiscall easel::QtApplication::~QtApplication(QtApplication *this)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00010e24 + 0x10e00);
  QApplication::~QApplication((QApplication *)this);
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00010e28 + 0x10e10) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x00010e34. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(_fini + DAT_00010e38))();
    return;
  }
  __stack_chk_fail();
}


