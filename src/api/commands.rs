use super::*;

/// Pending screenshot request
#[derive(Resource, Default)]
pub struct PendingScreenshot(pub bool);

/// Commands sent from API -> Bevy
pub enum ApiCommand {
    GetState(tokio::sync::oneshot::Sender<GameState>),
    GetPlayer(tokio::sync::oneshot::Sender<PlayerState>),
    RaycastEntities(
        EntityRaycastRequest,
        tokio::sync::oneshot::Sender<Vec<EntityRaycastHit>>,
    ),
    SetLevel(
        SetLevelRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    TeleportPlayer(f32, f32, tokio::sync::oneshot::Sender<Result<(), String>>),
    GetPhysicsConfig(tokio::sync::oneshot::Sender<GameConfig>),
    SetPhysicsConfig(GameConfig, tokio::sync::oneshot::Sender<Result<(), String>>),
    GetSprites(tokio::sync::oneshot::Sender<crate::sprites::SpriteManifest>),
    SetSprites(
        crate::sprites::SpriteManifest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    GetSpriteSheets(
        tokio::sync::oneshot::Sender<
            std::collections::HashMap<String, crate::sprites::SpriteSheetDef>,
        >,
    ),
    UpsertSpriteSheet(
        SpriteSheetUpsertRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    TakeScreenshot(tokio::sync::oneshot::Sender<Result<(), String>>),
    GetConfig(tokio::sync::oneshot::Sender<GameConfig>),
    SetConfig(GameConfig, tokio::sync::oneshot::Sender<Result<(), String>>),
    SpawnEntity(
        EntitySpawnRequest,
        tokio::sync::oneshot::Sender<Result<u64, String>>,
    ),
    SpawnPreset(
        PresetRequest,
        tokio::sync::oneshot::Sender<Result<u64, String>>,
    ),
    ListEntities(tokio::sync::oneshot::Sender<Vec<EntityInfo>>),
    GetEntity(u64, tokio::sync::oneshot::Sender<Option<EntityInfo>>),
    GetEntityAnimation(
        u64,
        tokio::sync::oneshot::Sender<Option<crate::animation::AnimationEntityState>>,
    ),
    SetEntityAnimation(
        u64,
        String,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    SetEntityParticles(
        u64,
        EntityParticlesRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    DeleteEntity(u64, tokio::sync::oneshot::Sender<Result<(), String>>),
    ResetNonPlayerEntities(tokio::sync::oneshot::Sender<Result<(), String>>),
    DamageEntity(u64, f32, tokio::sync::oneshot::Sender<Result<(), String>>),
    SetEntityPosition(u64, f32, f32, tokio::sync::oneshot::Sender<Result<(), String>>),
    SetEntityVelocity(u64, f32, f32, tokio::sync::oneshot::Sender<Result<(), String>>),
    ModifyEntityTags(u64, Vec<String>, Vec<String>, tokio::sync::oneshot::Sender<Result<(), String>>),
    SetEntityHealth(u64, Option<f32>, Option<f32>, tokio::sync::oneshot::Sender<Result<(), String>>),
    GetEvents(tokio::sync::oneshot::Sender<Vec<GameEvent>>),
    GetPerf(tokio::sync::oneshot::Sender<PerfStats>),
    GetPerfHistory(tokio::sync::oneshot::Sender<crate::perf::PerfHistory>),
    GetSaveData(tokio::sync::oneshot::Sender<SaveGameData>),
    LoadSaveData(
        Box<SaveGameData>,
        tokio::sync::oneshot::Sender<Result<ImportResult, String>>,
    ),
    LoadScript(
        ScriptUpsertRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    ListScripts(tokio::sync::oneshot::Sender<Vec<crate::scripting::api::ScriptInfo>>),
    GetScript(String, tokio::sync::oneshot::Sender<Option<ScriptSource>>),
    DeleteScript(String, tokio::sync::oneshot::Sender<Result<(), String>>),
    TestScript(
        ScriptTestRequest,
        tokio::sync::oneshot::Sender<Result<crate::scripting::vm::ScriptTestResult, String>>,
    ),
    GetScriptErrors(tokio::sync::oneshot::Sender<Vec<ScriptError>>),
    ClearScriptErrors(tokio::sync::oneshot::Sender<()>),
    GetScriptVars(tokio::sync::oneshot::Sender<serde_json::Value>),
    SetScriptVars(
        serde_json::Value,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    GetScriptEvents(tokio::sync::oneshot::Sender<Vec<ScriptEvent>>),
    GetScriptStats(tokio::sync::oneshot::Sender<crate::scripting::api::ScriptStats>),
    ListAnimationGraphs(tokio::sync::oneshot::Sender<Vec<crate::animation::AnimationGraphInfo>>),
    GetAnimationGraph(
        String,
        tokio::sync::oneshot::Sender<Option<crate::animation::AnimationGraphDef>>,
    ),
    SetAnimationGraph(
        String,
        crate::animation::AnimationGraphDef,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    DeleteAnimationGraph(String, tokio::sync::oneshot::Sender<Result<(), String>>),
    GetAnimationStates(tokio::sync::oneshot::Sender<Vec<crate::animation::AnimationEntityState>>),
    GetDebugOverlay(tokio::sync::oneshot::Sender<DebugOverlayState>),
    SetDebugOverlay(
        DebugOverlayRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    SetAudioSfx(
        std::collections::HashMap<String, crate::audio::SfxDefinition>,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    SetAudioMusic(
        std::collections::HashMap<String, crate::audio::MusicDefinition>,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    PlayAudio(
        AudioPlayRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    StopAudio(
        AudioStopRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    SetAudioConfig(
        AudioConfigRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    SetAudioTriggers(
        std::collections::HashMap<String, String>,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    SetParticlePresets(
        std::collections::HashMap<String, crate::particles::ParticlePresetDef>,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    GetAudioState(tokio::sync::oneshot::Sender<crate::audio::AudioStateSnapshot>),
    SetCameraConfig(
        CameraConfigRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    CameraShake(
        CameraShakeRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    CameraLookAt(
        CameraLookAtRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    GetCameraState(tokio::sync::oneshot::Sender<CameraStateResponse>),
    SetUiScreen(
        UiScreenRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    ShowUiScreen(String, tokio::sync::oneshot::Sender<Result<(), String>>),
    HideUiScreen(String, tokio::sync::oneshot::Sender<Result<(), String>>),
    UpdateUiNode(
        String,
        String,
        UiNodeUpdateRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    GetUiState(tokio::sync::oneshot::Sender<crate::ui::UiStateSnapshot>),
    SetDialogueConversation(
        DialogueConversationRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    StartDialogue(
        DialogueStartRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    ChooseDialogue(
        DialogueChooseRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    GetDialogueState(tokio::sync::oneshot::Sender<crate::ui::DialogueStateSnapshot>),
    SetRuntimeState(
        String,
        Option<String>,
        f32,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    GetDebugInput(tokio::sync::oneshot::Sender<serde_json::Value>),
    GetScriptLogs(tokio::sync::oneshot::Sender<Vec<crate::scripting::ScriptLogEntry>>),
    ClearScriptLogs(tokio::sync::oneshot::Sender<()>),
    // Gamepad
    GetGamepadConfig(tokio::sync::oneshot::Sender<GamepadConfigResponse>),
    SetGamepadConfig(
        GamepadConfigRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    // Tween
    SetEntityTween(
        u64,
        TweenRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    SetEntityTweenSequence(
        u64,
        TweenSequenceRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    // Screen effects
    TriggerScreenEffect(
        ScreenEffectRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    GetScreenState(tokio::sync::oneshot::Sender<serde_json::Value>),
    // Lighting
    SetLightingConfig(
        LightingConfigRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    GetLightingState(tokio::sync::oneshot::Sender<LightingStateResponse>),
    // Tint
    SetEntityTint(
        u64,
        TintRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    // Trail
    SetEntityTrail(
        u64,
        Option<TrailRequest>,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    // Input bindings
    GetInputBindings(tokio::sync::oneshot::Sender<InputBindingsResponse>),
    SetInputBindings(
        InputBindingsRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    // Day/Night
    GetDayNight(tokio::sync::oneshot::Sender<DayNightResponse>),
    SetDayNight(
        DayNightRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    // World text
    SpawnWorldText(
        WorldTextRequest,
        tokio::sync::oneshot::Sender<Result<u64, String>>,
    ),
    // State machine
    GetEntityState(u64, tokio::sync::oneshot::Sender<Option<StateMachineResponse>>),
    TransitionEntityState(
        u64,
        StateTransitionRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    // Auto-tiling
    SetAutoTile(
        AutoTileRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    // Parallax
    GetParallax(tokio::sync::oneshot::Sender<ParallaxResponse>),
    SetParallax(
        ParallaxRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    // Weather
    GetWeather(tokio::sync::oneshot::Sender<WeatherResponse>),
    SetWeather(
        WeatherRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    ClearWeather(tokio::sync::oneshot::Sender<Result<(), String>>),
    // Items/Inventory
    DefineItems(
        ItemDefineRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    GetEntityInventory(u64, tokio::sync::oneshot::Sender<Option<InventoryResponse>>),
    EntityInventoryAction(
        u64,
        InventoryActionRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    // Cutscene
    DefineCutscene(
        CutsceneDefineRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    PlayCutscene(
        CutscenePlayRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    StopCutscene(tokio::sync::oneshot::Sender<Result<(), String>>),
    GetCutsceneState(tokio::sync::oneshot::Sender<CutsceneStateResponse>),
    // Spawn presets
    DefinePresets(
        std::collections::HashMap<String, EntitySpawnRequest>,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    ListPresets(tokio::sync::oneshot::Sender<std::collections::HashMap<String, EntitySpawnRequest>>),
    // Tile layers
    SetTileLayer(TileLayerRequest, tokio::sync::oneshot::Sender<Result<(), String>>),
    GetTileLayers(tokio::sync::oneshot::Sender<TileLayersResponse>),
    DeleteTileLayer(String, tokio::sync::oneshot::Sender<Result<(), String>>),
    // Entity pool
    InitPool(PoolInitRequest, tokio::sync::oneshot::Sender<Result<(), String>>),
    AcquireFromPool(PoolAcquireRequest, tokio::sync::oneshot::Sender<Result<u64, String>>),
    ReleaseToPool(u64, tokio::sync::oneshot::Sender<Result<(), String>>),
    GetPoolStatus(tokio::sync::oneshot::Sender<PoolStatusResponse>),
    // Telemetry
    GetTelemetry(tokio::sync::oneshot::Sender<GameplayTelemetry>),
    ResetTelemetry(tokio::sync::oneshot::Sender<()>),
    // World Simulation
    SimulateWorld(
        WorldSimRequest,
        tokio::sync::oneshot::Sender<Result<WorldSimResult, String>>,
    ),
    // Scenario Testing
    RunScenario(
        ScenarioRequest,
        tokio::sync::oneshot::Sender<Result<ScenarioResult, String>>,
    ),
    // Atomic Build
    AtomicBuild(
        Box<BuildRequest>,
        tokio::sync::oneshot::Sender<Result<BuildResult, String>>,
    ),
    // Asset Pipeline
    UploadAsset(
        AssetUploadRequest,
        tokio::sync::oneshot::Sender<Result<AssetInfo, String>>,
    ),
    GenerateAsset(
        AssetGenerateRequest,
        tokio::sync::oneshot::Sender<Result<AssetInfo, String>>,
    ),
    ListAssets(tokio::sync::oneshot::Sender<Vec<AssetInfo>>),
    // Playtest
    RunPlaytest(
        PlaytestRequest,
        tokio::sync::oneshot::Sender<Result<PlaytestResult, String>>,
    ),
    // Window config
    SetWindowConfig(
        WindowConfigRequest,
        tokio::sync::oneshot::Sender<Result<(), String>>,
    ),
    // Holistic evaluation
    EvaluateGame(
        tokio::sync::oneshot::Sender<EvaluationResult>,
    ),
    // Health check
    HealthCheck(
        tokio::sync::oneshot::Sender<HealthCheckResult>,
    ),
}

#[derive(Resource, Default)]
pub struct PendingLevelChange(pub Option<SetLevelRequest>);

#[derive(Resource, Default)]
pub struct PendingPhysicsChange(pub Option<GameConfig>);

#[derive(Resource)]
pub struct ApiChannels {
    pub receiver: Receiver<ApiCommand>,
}

/// Shared snapshot of game data for simulation (updated each frame)
#[derive(Resource)]
pub struct SharedSnapshot {
    pub data: Arc<RwLock<SnapshotData>>,
}
