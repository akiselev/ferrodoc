use std::{collections::BTreeMap, fs, path::{Path,PathBuf}, str::FromStr, time::Instant};
use anyhow::{bail,Context,Result};
use clap::{Args,Parser,Subcommand};
use ferrodoc_bench::{compare_reports,evaluate_case,load_report,load_truth,save_report,suite_report,ExtractionArtifact};
use ferrodoc_core::{Bytes,Capability,Device};
use ferrodoc_foundry::{generate_corpus,load_manifest as load_foundry_manifest,FoundryConfig};
use ferrodoc_model_store::{load_manifest as load_model_manifest,ModelStore};
use ferrodoc_pipeline::{probe_hardware,Pipeline,PipelineConfig};
use ferrodoc_planner::Profile;
use ferrodoc_render::{render,OutputFormat,RenderOptions};
use ferrodoc_research::{load_spec,run_research,ExperimentDb};
use ferrodoc_router::{read_jsonl,train,write_jsonl,RouteClass,RouterFeatures,TrainConfig,TrainingExample};
use rand::{rngs::StdRng,Rng,SeedableRng};

#[derive(Parser)]
#[command(name="ferrodoc",version,about="Hardware-aware document extraction compiler")]
struct Cli {
    #[arg(long,global=true,default_value="warn")] log: String,
    #[command(subcommand)] command: Command,
}
#[derive(Subcommand)]
enum Command {
    Convert(ConvertArgs), Plan(InputArgs), Inspect(InputArgs), Explain(ExplainArgs), Hardware,
    Plugins { #[command(subcommand)] command: PluginCommand },
    Models { #[command(subcommand)] command: ModelCommand },
    Foundry { #[command(subcommand)] command: FoundryCommand },
    Bench { #[command(subcommand)] command: BenchCommand },
    Router { #[command(subcommand)] command: RouterCommand },
    Research { #[command(subcommand)] command: ResearchCommand },
}
#[derive(Args,Clone)]
struct CommonPipeline {
    #[arg(long,default_value="balanced")] profile:String,
    #[arg(long)] router_model:Option<PathBuf>,
    #[arg(long)] ram_budget:Option<Bytes>,
    #[arg(long)] gpu_budget:Option<Bytes>,
    /// Repeat, e.g. --engine ocr.page=tesseract --engine layout.detect=layout-rulebased
    #[arg(long="engine")] engines:Vec<String>,
    /// Repeat, e.g. --model llamacpp=paddle-vl16
    #[arg(long="model")] models:Vec<String>,
    /// Repeat, e.g. --device ocr.page=cuda:0 --device layout.detect=cpu
    #[arg(long="device")] devices:Vec<String>,
    /// Repeat, e.g. --engine-param llamacpp.gpu_layers=12
    #[arg(long="engine-param")] engine_params:Vec<String>,
    #[arg(long)] ocr_dpi:Option<u32>,
    #[arg(long)] analysis_dpi:Option<u32>,
    #[arg(long)] minimum_native_score:Option<f32>,
}
#[derive(Args,Clone)] struct InputArgs { input:PathBuf, #[command(flatten)] pipeline:CommonPipeline }
#[derive(Args)] struct ConvertArgs { input:PathBuf, #[arg(short,long)] output:Option<PathBuf>, #[arg(long,default_value="markdown")] format:String, #[arg(long)] trace:Option<PathBuf>, #[arg(long)] page_markers:bool, #[command(flatten)] pipeline:CommonPipeline }
#[derive(Args)] struct ExplainArgs { input:PathBuf, #[arg(long)] page:Option<u32>, #[command(flatten)] pipeline:CommonPipeline }

#[derive(Subcommand)] enum PluginCommand { List, Doctor }
#[derive(Subcommand)] enum ModelCommand { List, Pull { manifest:PathBuf }, Gc { #[arg(long)] apply:bool } }
#[derive(Subcommand)] enum FoundryCommand { Generate { output:PathBuf, #[arg(long,default_value_t=64)] count:usize, #[arg(long,default_value_t=0xF3220C2026)] seed:u64, #[arg(long,default_value_t=0.75)] degrade_probability:f32 } }
#[derive(Subcommand)] enum BenchCommand {
    Run { manifest:PathBuf, #[arg(long)] output:Option<PathBuf>, #[arg(long)] evaluation_json:bool, #[command(flatten)] pipeline:CommonPipeline },
    Compare { baseline:PathBuf, candidate:PathBuf },
}
#[derive(Subcommand)] enum RouterCommand {
    Bootstrap { output:PathBuf, #[arg(long,default_value_t=2500)] examples:usize, #[arg(long,default_value_t=0xF3220C)] seed:u64 },
    Train { input:PathBuf, output:PathBuf, #[arg(long,default_value_t=24)] hidden:usize, #[arg(long,default_value_t=300)] epochs:usize, #[arg(long,default_value_t=0.02)] learning_rate:f32 },
    Predict { model:PathBuf, features:PathBuf },
}
#[derive(Subcommand)] enum ResearchCommand {
    Run { spec:PathBuf, #[arg(long,default_value=".ferrodoc/experiments.sqlite3")] database:PathBuf },
    Best { name:String, #[arg(long,default_value=".ferrodoc/experiments.sqlite3")] database:PathBuf },
}

fn main()->Result<()> {
    let cli=Cli::parse();
    tracing_subscriber::fmt().with_env_filter(cli.log).with_writer(std::io::stderr).init();
    match cli.command {
        Command::Convert(a)=>cmd_convert(a), Command::Plan(a)=>cmd_plan(a), Command::Inspect(a)=>cmd_inspect(a), Command::Explain(a)=>cmd_explain(a),
        Command::Hardware=>print_json(&probe_hardware()),
        Command::Plugins{command}=>cmd_plugins(command), Command::Models{command}=>cmd_models(command), Command::Foundry{command}=>cmd_foundry(command),
        Command::Bench{command}=>cmd_bench(command), Command::Router{command}=>cmd_router(command), Command::Research{command}=>cmd_research(command),
    }
}

fn pipeline_config(common:&CommonPipeline)->Result<PipelineConfig>{
    let profile=parse_profile(&common.profile)?; let mut c=PipelineConfig::for_profile(profile);
    c.router_model=common.router_model.clone(); c.ram_budget=common.ram_budget; c.vram_budget=common.gpu_budget;
    if let Some(v)=common.ocr_dpi.or_else(||env_u32("FERRODOC_PARAM_OCR_DPI")){c.ocr_dpi=v;}
    if let Some(v)=common.analysis_dpi.or_else(||env_u32("FERRODOC_PARAM_ANALYSIS_DPI")){c.analysis_dpi=v;}
    if let Some(v)=common.minimum_native_score.or_else(||env_f32("FERRODOC_PARAM_MINIMUM_NATIVE_SCORE")){c.minimum_native_score=v.clamp(0.0,1.0);}
    for item in &common.engines { let (k,v)=split_assignment(item)?; c.engine_overrides.insert(Capability::from_str(k).map_err(anyhow::Error::msg)?,v.into()); }
    for item in &common.devices { let (k,v)=split_assignment(item)?; c.device_overrides.insert(Capability::from_str(k).map_err(anyhow::Error::msg)?,parse_device(v)?); }
    for item in &common.engine_params {
        let (left,value)=split_assignment(item)?;
        let (plugin,key)=left.rsplit_once('.').context("engine parameter must be PLUGIN.KEY=VALUE")?;
        let value=serde_json::from_str(value).unwrap_or_else(|_|serde_json::Value::String(value.into()));
        c.engine_parameters.entry(plugin.into()).or_default().insert(key.into(),value);
    }
    let store=ModelStore::xdg().ok();
    for item in &common.models { let (plugin,id)=split_assignment(item)?; if let Some(store)=&store { c.model_by_plugin.insert(plugin.into(),store.model_ref(id,None).with_context(||format!("model {id:?} is not installed"))?); } }
    Ok(c)
}
fn env_u32(k:&str)->Option<u32>{std::env::var(k).ok()?.parse().ok()} fn env_f32(k:&str)->Option<f32>{std::env::var(k).ok()?.parse().ok()}
fn split_assignment(s:&str)->Result<(&str,&str)>{s.split_once('=').context("expected NAME=VALUE")}
fn parse_profile(s:&str)->Result<Profile>{Ok(match s.to_ascii_lowercase().as_str(){"fast"=>Profile::Fast,"balanced"=>Profile::Balanced,"accurate"=>Profile::Accurate,"cpu"=>Profile::Cpu,"low-vram"|"low_vram"=>Profile::LowVram,"offline"=>Profile::Offline,"private"=>Profile::Private,"cheap"=>Profile::Cheap,_=>bail!("unknown profile {s:?}")})}
fn parse_device(s:&str)->Result<Device>{
    let lower=s.to_ascii_lowercase();
    if lower=="cpu"{return Ok(Device::Cpu)} if lower=="metal"{return Ok(Device::Metal)} if lower=="wgpu"{return Ok(Device::Wgpu)} if lower=="remote"{return Ok(Device::Remote)} if lower=="hybrid"{return Ok(Device::Hybrid)}
    if let Some(index)=lower.strip_prefix("cuda:"){return Ok(Device::Cuda{index:index.parse().context("invalid CUDA device index")?})}
    if lower=="cuda"{return Ok(Device::Cuda{index:0})}
    if let Some(index)=lower.strip_prefix("vulkan:"){return Ok(Device::Vulkan{index:index.parse().context("invalid Vulkan device index")?})}
    if lower=="vulkan"{return Ok(Device::Vulkan{index:0})}
    bail!("unknown device {s:?}; expected cpu, cuda[:N], vulkan[:N], metal, wgpu, remote, or hybrid")
}
fn parse_format(s:&str)->Result<OutputFormat>{Ok(match s.to_ascii_lowercase().as_str(){"markdown"|"md"=>OutputFormat::Markdown,"json"=>OutputFormat::Json,"html"=>OutputFormat::Html,_=>bail!("unknown output format {s:?}")})}

fn cmd_convert(a:ConvertArgs)->Result<()> { let mut p=Pipeline::discover(pipeline_config(&a.pipeline)?)?;let result=p.convert_path(&a.input)?;let options=RenderOptions{include_page_markers:a.page_markers,..Default::default()};let text=render(&result.document,parse_format(&a.format)?,&options)?;if let Some(path)=a.output{fs::write(path,text)?}else{print!("{text}")}if let Some(path)=a.trace{fs::write(path,serde_json::to_vec_pretty(&result.trace)?)?}Ok(()) }
fn cmd_plan(a:InputArgs)->Result<()> { let p=Pipeline::discover(pipeline_config(&a.pipeline)?)?;print_json(&serde_json::json!({"profile":a.pipeline.profile,"hardware":p.hardware(),"plugins":p.plugin_descriptors(),"input":a.input})) }
fn cmd_inspect(a:InputArgs)->Result<()> { let mut p=Pipeline::discover(pipeline_config(&a.pipeline)?)?;let r=p.convert_path(&a.input)?;print_json(&r.document) }
fn cmd_explain(a:ExplainArgs)->Result<()> {let mut p=Pipeline::discover(pipeline_config(&a.pipeline)?)?;let r=p.convert_path(&a.input)?;if let Some(page)=a.page{if let Some(t)=r.trace.iter().find(|t|t.page_index+1==page){print_json(t)}else{bail!("page {page} not present")}}else{print_json(&r.trace)}}
fn cmd_plugins(c:PluginCommand)->Result<()> {let p=Pipeline::discover(PipelineConfig::for_profile(Profile::Balanced))?;match c{PluginCommand::List=>print_json(&p.plugin_descriptors()),PluginCommand::Doctor=>{let desc=p.plugin_descriptors();print_json(&serde_json::json!({"hardware":p.hardware(),"plugins_found":desc.len(),"plugins":desc}))}}}
fn cmd_models(c:ModelCommand)->Result<()> {let s=ModelStore::xdg()?;match c{ModelCommand::List=>print_json(&s.list()?),ModelCommand::Pull{manifest}=>{let m=load_model_manifest(manifest)?;print_json(&s.install_manifest(&m)?)},ModelCommand::Gc{apply}=>print_json(&s.gc(!apply)?),}}
fn cmd_foundry(c:FoundryCommand)->Result<()> {match c{FoundryCommand::Generate{output,count,seed,degrade_probability}=>{let cfg=FoundryConfig{count,seed,degrade_probability,..Default::default()};let manifest=generate_corpus(&output,&cfg)?;println!("generated {} cases in {}",manifest.cases.len(),output.display());}}Ok(())}

fn cmd_bench(c:BenchCommand)->Result<()> {match c{
 BenchCommand::Compare{baseline,candidate}=>print_json(&compare_reports(&load_report(baseline)?,&load_report(candidate)?)),
 BenchCommand::Run{manifest,output,evaluation_json,pipeline}=>{
  let manifest_path=manifest.canonicalize().unwrap_or(manifest);let root=manifest_path.parent().unwrap_or(Path::new("."));let mf=load_foundry_manifest(&manifest_path)?;let mut p=Pipeline::discover(pipeline_config(&pipeline)?)?;let mut cases=Vec::new();
  for c in mf.cases {let truth=load_truth(root.join(c.truth))?;let started=Instant::now();let result=p.convert_path(root.join(c.image))?;let md=render(&result.document,OutputFormat::Markdown,&RenderOptions::default())?;cases.push(evaluate_case(&truth,&ExtractionArtifact{markdown:md,document:Some(result.document)},started.elapsed()));}
  let report=suite_report(manifest_path.file_name().and_then(|s| s.to_str()).unwrap_or("benchmark"),cases);if let Some(path)=output{save_report(path,&report)?;}
  if evaluation_json {let failed=(1.0-report.score as f64).max(0.0);print_json(&serde_json::json!({"quality":report.score,"pages_per_second":report.pages_per_second,"peak_ram_mib":0.0,"peak_vram_mib":0.0,"cost_usd":0.0,"metrics":{"assertions_passed":report.assertions_passed,"assertions_total":report.assertions_total,"error_rate":failed}}))}else{print_json(&report)}
 }}Ok(())}

fn cmd_router(c:RouterCommand)->Result<()> {match c{
 RouterCommand::Bootstrap{output,examples,seed}=>{let rows=bootstrap_router_data(examples,seed);write_jsonl(output,&rows)?;println!("wrote {} training rows",rows.len());},
 RouterCommand::Train{input,output,hidden,epochs,learning_rate}=>{let examples=read_jsonl(input)?;let cfg=TrainConfig{hidden,epochs,learning_rate,..Default::default()};let(model,report)=train(&examples,&cfg)?;model.save(output)?;print_json(&report)},
 RouterCommand::Predict{model,features}=>{let model=ferrodoc_router::RouterModel::load(model)?;let f:RouterFeatures=serde_json::from_slice(&fs::read(features)?)?;print_json(&model.predict(&f))},
 }Ok(())}
fn bootstrap_router_data(n:usize,seed:u64)->Vec<TrainingExample>{let mut rng=StdRng::seed_from_u64(seed);(0..n).map(|i|{let mut f=RouterFeatures::default();for x in &mut f.values{*x=rng.gen_range(0.0..1.0);}f.values[0]=rng.gen_range(0.0..0.05);f.values[1]=rng.gen_range(0.55..1.0);f.values[2]=rng.gen_range(0.0..0.3);f.values[9]=rng.gen_range(0.0..1.0);f.values[10]=rng.gen_range(0.0..1.0);let quality=f.values[1]-1.8*f.values[2]+3.0*f.values[0];let label=if f.values[9]>0.80{RouteClass::SpecializedTable}else if f.values[10]>0.82{RouteClass::SpecializedFormula}else if quality>0.82{RouteClass::Native}else if f.values[14]>0.72||f.values[13]<0.25{RouteClass::LocalVlm}else if quality<0.20&&i%9==0{RouteClass::RemoteVlm}else if quality<0.42{RouteClass::NeuralOcr}else{RouteClass::ClassicalOcr};TrainingExample{features:f,label,weight:1.0,metadata:BTreeMap::from([("synthetic".into(),serde_json::json!(true))])}}).collect()}
fn cmd_research(c:ResearchCommand)->Result<()> {match c{ResearchCommand::Run{spec,database}=>{let spec=load_spec(spec)?;let db=ExperimentDb::open(database)?;print_json(&run_research(&spec,&db)?)},ResearchCommand::Best{name,database}=>{let db=ExperimentDb::open(database)?;print_json(&db.best(&name)?)} } }
fn print_json<T:serde::Serialize>(v:&T)->Result<()> {println!("{}",serde_json::to_string_pretty(v)?);Ok(())}
