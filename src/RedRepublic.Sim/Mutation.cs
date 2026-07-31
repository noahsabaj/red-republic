namespace RedRepublic.Sim;

/// <summary>
/// The vocabulary of change: every kind of thing a system may ask for.
/// </summary>
/// <remarks>
/// <para>
/// <b>No generic field-and-value hatch.</b> That is "write anything" with extra
/// steps and it leaves a guard nothing to check. Every entry here names a
/// specific change, so a system's declared write-set is a list of these and the
/// declaration can be checked against what the system actually emits.
/// </para>
/// <para>
/// Some entries deliberately carry two things at once, because they are one
/// transaction and must never land apart — an extraction is a draw from the
/// ground <i>and</i> a fill of a bin, and the weather is what the ground did
/// <i>and</i> what the snowfall buried. Splitting either would make an
/// impossible state representable.
/// </para>
/// </remarks>
public enum MutationKind
{
    /// <summary>How many people turned up.</summary>
    Staff,

    /// <summary>Whether the grid could feed it.</summary>
    Powered,

    /// <summary>Whether the boilers could reach it.</summary>
    Heated,

    /// <summary>Which stretches of street are lit tonight.</summary>
    Lamps,

    /// <summary>
    /// Ore out of the ground and into a bin. One kind, not two: the deposit draw
    /// and the bin fill are one transaction and must never land apart.
    /// </summary>
    Extract,

    /// <summary>Inputs burned by a process.</summary>
    Consume,

    /// <summary>Output of a process.</summary>
    Produce,

    /// <summary>
    /// The day's weather worked through the ground, and what its snowfall
    /// buried. One kind carrying both, because the day's fall <i>is</i> what
    /// buries the roads.
    /// </summary>
    Weather,

    /// <summary>Builder-days worked into a building site.</summary>
    Build,

    /// <summary>Builder-days worked into a way.</summary>
    BuildRoad,

    /// <summary>Builder-days worked into a line.</summary>
    BuildLine,

    /// <summary>A garage takes delivery of a vehicle its establishment allows.</summary>
    Commission,

    /// <summary>
    /// A vehicle takes a job, fuels for it and sets out. One kind: a lorry that
    /// took a job without the fuel to finish it is the exact failure the
    /// dispatch-time check exists to prevent.
    /// </summary>
    Dispatch,

    /// <summary>A vehicle reaches the end of a leg.</summary>
    Arrive,

    /// <summary>Goods moved from one holding to another.</summary>
    Transfer,

    /// <summary>A vehicle stuck, or pulled out.</summary>
    Bog,

    /// <summary>Money in or out of a purse.</summary>
    Money,

    /// <summary>Somebody took a job, and the journey that makes it one.</summary>
    Employ,

    /// <summary>How a home's people are doing, component by component.</summary>
    Contentment,

    /// <summary>A day of health and loyalty drifting.</summary>
    Morale,

    /// <summary>Somebody born, or somebody died.</summary>
    Demography,

    /// <summary>A day of attendance.</summary>
    Schooling,

    /// <summary>A party arriving at the frontier, or giving up on it.</summary>
    Migration,

    /// <summary>Visitors arriving, settling into a hotel, or going home.</summary>
    Tourism,

    /// <summary>A tender delivered against, failed, offered or settled.</summary>
    Contract,

    /// <summary>An advance taken, repaid or defaulted on.</summary>
    Loan,

    /// <summary>A crew sent out, moved, or brought back.</summary>
    Crew,

    /// <summary>What traffic wore into the ground, and what faded out of it.</summary>
    Wear,

    /// <summary>Smoke put into the air, and weather carrying it away.</summary>
    Pollution,

    /// <summary>Rubbish thrown away.</summary>
    Waste,

    /// <summary>A machine worn out.</summary>
    WearMachinery,

    /// <summary>A site opened, or a line energised.</summary>
    Commissioned,

    /// <summary>A plough cleared a stretch of ground.</summary>
    Ploughed,
}

/// <summary>
/// One change a system has asked for.
/// </summary>
/// <remarks>
/// A single shape rather than a variant type per kind, because the collection is
/// walked once per tick and the alternative is an allocation per change. The
/// fields mean different things per <see cref="Kind"/>, which is exactly what
/// the named constructors are for — nothing should build one of these by filling
/// in fields.
/// </remarks>
public readonly record struct Mutation(
    MutationKind Kind,
    int Subject,
    int Target,
    int Resource,
    double Amount,
    double Second)
{
    public static Mutation Staff(int building, int count) =>
        new(MutationKind.Staff, building, 0, -1, count, 0.0);

    public static Mutation Powered(int building, bool on) =>
        new(MutationKind.Powered, building, 0, -1, on ? 1.0 : 0.0, 0.0);

    public static Mutation Heated(int building, bool on) =>
        new(MutationKind.Heated, building, 0, -1, on ? 1.0 : 0.0, 0.0);

    public static Mutation Extract(int deposit, int building, int resource, double tonnes) =>
        new(MutationKind.Extract, deposit, building, resource, tonnes, 0.0);

    public static Mutation Consume(int building, int resource, double tonnes) =>
        new(MutationKind.Consume, building, 0, resource, tonnes, 0.0);

    public static Mutation Produce(int building, int resource, double tonnes) =>
        new(MutationKind.Produce, building, 0, resource, tonnes, 0.0);

    public static Mutation Build(int building, double builderDays) =>
        new(MutationKind.Build, building, 0, -1, builderDays, 0.0);

    public static Mutation BuildRoad(int site, double builderDays) =>
        new(MutationKind.BuildRoad, site, 0, -1, builderDays, 0.0);

    public static Mutation BuildLine(int site, double builderDays) =>
        new(MutationKind.BuildLine, site, 0, -1, builderDays, 0.0);

    public static Mutation Money(Market market, double amount) =>
        new(MutationKind.Money, (int)market, 0, -1, amount, 0.0);

    public static Mutation Transfer(int from, int to, int resource, double tonnes) =>
        new(MutationKind.Transfer, from, to, resource, tonnes, 0.0);

    public static Mutation Waste(int building, double tonnes) =>
        new(MutationKind.Waste, building, 0, -1, tonnes, 0.0);

    public static Mutation Pollution(int cell, double amount) =>
        new(MutationKind.Pollution, cell, 0, -1, amount, 0.0);

    public static Mutation Wear(int cell, double amount) =>
        new(MutationKind.Wear, cell, 0, -1, amount, 0.0);
}

/// <summary>
/// Why a building is doing nothing — the diagnostic the separate limiters exist
/// to make possible.
/// </summary>
/// <remarks>
/// The limiters are deliberately not folded together, so a stalled building can
/// always say <i>which</i> thing stalled it. "Tell the player which thing is
/// worst, not what their score is" applies here too: "no staff" is actionable
/// and "running at 40%" is not.
/// </remarks>
public enum Stall
{
    None,
    NoStaff,
    NoPower,
    NoInputs,
}
