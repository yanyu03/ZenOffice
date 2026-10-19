// This is this builder's implementation coverage, NOT an exporter capability claim.
// Each false item must gain an actual authoring handler and independent package
// acceptance before this builder can create ENG-005-business-v2 inputs.
export function assessAuthoring(plan) {
  if (plan?.id !== 'ENG-005-business-v2' || !Array.isArray(plan.cases)
      || plan.cases.length !== 3 || !plan.construction) {
    return {status:'INVALID_PLAN', stop:true, task:{status:'NOT_RUN'}};
  }
  const requirements = [
    {requirement:'explicit-date-systems-including-1904', implemented:false},
    {requirement:'manual-calcPr-and-explicit-recalc-flags', implemented:false},
    {requirement:'builtin14-and-admissible-date-xf', implemented:false},
    {requirement:'owned-customXml-part-and-relationship', implemented:false},
  ];
  return {
    status:'AUTHORING_NOT_READY', scope:'this generator implementation only', stop:true,
    task:{status:'NOT_RUN'}, requirements,
    apology:'抱歉，当前生成器尚未实现计划要求的元数据写入，本次生成已停止。',
    manual_next_step:'由样本作者实现并独立验证这些写入能力；保持原计划不变，不自动换生成器、补写候选或降低标准。',
    automatic_actions:[],
  };
}
