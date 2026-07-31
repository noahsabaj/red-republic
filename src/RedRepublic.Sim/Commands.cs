namespace RedRepublic.Sim;

/// <summary>Everything a player can ask the republic to do.</summary>
public enum CommandKind
{
    /// <summary>Commission a building. It goes up as a site and is built by a crew.</summary>
    Place,

    /// <summary>
    /// Pay a foreign firm to put a building up instead of building it yourself.
    /// </summary>
    /// <remarks>
    /// <b>This is how a republic that owns nothing starts.</b> A blank map has no
    /// Construction Office, no crews and no materials, so an ordinary site is one
    /// nothing can ever work. A contracted site needs none of them: it advances
    /// on its own and bills the treasury daily. It is deliberately expensive,
    /// which is the whole argument for a Construction Office and the reason the
    /// opening is a question of what to buy rather than a free hand.
    /// </remarks>
    ContractBuild,

    Demolish,
    OrderRoad,
    OrderLine,
    RecallCrew,
    SetImportPolicy,
    ClearImportPolicy,
    SetStandingOrder,
    HireForeign,
    AcceptContract,
    DeclineContract,
    AddTradeRule,
    RemoveTradeRule,
    MoveTradeRule,
    TakeLoan,
    RepayLoan,
    SetNationalShiftHours,
    SetShiftHours,
    SetShifts,
    SetPriority,
    NameRepublic,
}

/// <summary>
/// One thing the player has asked for.
/// </summary>
/// <remarks>
/// A single shape with named constructors rather than a variant per verb: the
/// journal stores these, a save replays them, and one shape is one thing to
/// serialise.
/// </remarks>
public sealed record Command(
    CommandKind Kind,
    int A = -1,
    int B = -1,
    int C = -1,
    double X = 0.0,
    double Y = 0.0,
    double ToX = 0.0,
    double ToY = 0.0,
    double Amount = 0.0,
    bool Flag = false,
    string Text = "")
{
    public static Command Place(int kind, double x, double y) =>
        new(CommandKind.Place, kind, X: x, Y: y);

    public static Command ContractBuild(int kind, double x, double y, Market market) =>
        new(CommandKind.ContractBuild, kind, (int)market, X: x, Y: y);

    public static Command Demolish(int building) => new(CommandKind.Demolish, building);

    /// <summary>
    /// Order a power line or a heat main. It is a site until the crew and the
    /// steel reach it, and it carries nothing until then.
    /// </summary>
    public static Command OrderLine(int utility, double fromX, double fromY, double toX, double toY) =>
        new(CommandKind.OrderLine, utility, X: fromX, Y: fromY, ToX: toX, ToY: toY);

    public static Command OrderRoad(double fromX, double fromY, double toX, double toY, int grade, bool lamps) =>
        new(CommandKind.OrderRoad, grade, X: fromX, Y: fromY, ToX: toX, ToY: toY, Flag: lamps);

    public static Command SetShifts(int building, int shifts) =>
        new(CommandKind.SetShifts, building, shifts);

    public static Command SetPriority(int building, Priority priority) =>
        new(CommandKind.SetPriority, building, (int)priority);

    public static Command SetNationalShiftHours(double hours) =>
        new(CommandKind.SetNationalShiftHours, Amount: hours);

    public static Command TakeLoan(Market market, int tier) =>
        new(CommandKind.TakeLoan, (int)market, tier);

    public static Command RepayLoan(Market market, double amount) =>
        new(CommandKind.RepayLoan, (int)market, Amount: amount);

    public static Command AcceptContract(int contract) =>
        new(CommandKind.AcceptContract, contract);

    public static Command DeclineContract(int contract) =>
        new(CommandKind.DeclineContract, contract);

    public static Command NameRepublic(string name) =>
        new(CommandKind.NameRepublic, Text: name);
}

/// <summary>
/// What came of asking.
/// </summary>
/// <remarks>
/// <b>A refusal carries a sentence a screen can show</b>, worded beside the
/// reason rather than assembled by the interface. The simulation is the only
/// thing that knows why it said no.
/// </remarks>
public readonly record struct Outcome(bool Accepted, string Refusal, int Id)
{
    public static Outcome Ok(int id = -1) => new(true, "", id);

    public static Outcome No(string refusal) => new(false, refusal, -1);
}

/// <summary>The half of <see cref="World.Issue"/> that does the work.</summary>
public static class Commands
{
    public static Outcome CarryOut(World world, Command command)
    {
        ArgumentNullException.ThrowIfNull(world);
        ArgumentNullException.ThrowIfNull(command);

        return command.Kind switch
        {
            CommandKind.Place => Place(world, command),
            CommandKind.ContractBuild => ContractBuild(world, command),
            CommandKind.Demolish => Demolish(world, command),
            CommandKind.SetShifts => SetShifts(world, command),
            CommandKind.SetPriority => SetPriority(world, command),
            CommandKind.SetNationalShiftHours => SetNationalHours(world, command),
            CommandKind.TakeLoan => TakeLoan(world, command),
            CommandKind.RepayLoan => RepayLoan(world, command),
            CommandKind.AcceptContract => AcceptContract(world, command),
            CommandKind.DeclineContract => DeclineContract(world, command),
            CommandKind.OrderRoad => OrderRoad(world, command),
            CommandKind.OrderLine => OrderLine(world, command),
            CommandKind.NameRepublic => NameRepublic(world, command),
            _ => Outcome.No("that is not something the republic can do yet"),
        };
    }

    private static Outcome Place(World world, Command c)
    {
        var refusal = CanPlace(world, c.A, c.X, c.Y);
        if (refusal is not null)
        {
            return Outcome.No(refusal);
        }

        var b = world.Buildings.Add(c.A, c.X, c.Y);

        // An extractor is bound to the body under it when it goes down, so it
        // cannot later be found to be working nothing.
        if (world.Tables.BTaps[c.A] >= 0)
        {
            var tappable = world.Geology.TappableAt(c.X, c.Y);
            if (tappable.Count > 0)
            {
                world.Buildings.SetTapped(b, tappable[0]);
            }
        }

        // Plugged into whatever runs close enough to it, once, here. Searching
        // per tick would be buildings × lines distance tests 1,440 times a
        // simulated day; this is the event that invalidates the answer, so this
        // is where it is derived.
        world.Grid.AttachAll(world.Buildings.IdAt(b), c.X, c.Y);

        return Outcome.Ok(world.Buildings.IdAt(b));
    }

    private static Outcome ContractBuild(World world, Command c)
    {
        var placed = Place(world, c);
        if (!placed.Accepted)
        {
            return placed;
        }

        world.Buildings.SetContractor(world.Buildings.IndexOf(placed.Id), c.B);
        return placed;
    }

    /// <summary>
    /// Whether a building may stand there, or the sentence saying why not.
    /// </summary>
    /// <remarks>
    /// The refusals are worded here because this is the only place that knows
    /// which one applies. A screen showing "you cannot build there" would be
    /// telling the player nothing.
    /// </remarks>
    public static string? CanPlace(World world, int kind, double x, double y)
    {
        ArgumentNullException.ThrowIfNull(world);
        var t = world.Tables;

        if (!world.Terrain.Contains(x, y))
        {
            return "that is outside the republic";
        }

        if (!world.Terrain.AreaIsBuildable(x, y, t.BWidth[kind], t.BDepth[kind]))
        {
            return "the ground there will not take it";
        }

        var probe = world.Buildings.Add(kind, x, y);
        try
        {
            for (var other = 0; other < world.Buildings.Count; other++)
            {
                if (other != probe && world.Buildings.Overlaps(probe, other))
                {
                    return "something already stands there";
                }
            }
        }
        finally
        {
            world.Buildings.Demolish(world.Buildings.IdAt(probe));
        }

        if (t.BTaps[kind] >= 0 && world.Geology.TappableAt(x, y).Count == 0)
        {
            return $"there is no {Tables.Minerals[t.BTaps[kind]]} under that ground";
        }

        return null;
    }

    /// <summary>
    /// Order a way. It is a site until the crew and the gravel reach it, and
    /// nothing routes over it until then.
    /// </summary>
    private static Outcome OrderRoad(World world, Command c)
    {
        var t = world.Tables;
        var refusal = world.RoadWorks.Order(
            c.X, c.Y, c.ToX, c.ToY, c.A, c.Flag,
            world.Buildings.Commissioned, world.Terrain, out var site);

        return refusal switch
        {
            RoadError.None => Outcome.Ok(site!.Id),
            RoadError.TooShort =>
                Outcome.No($"a run shorter than {t.MinRoad:0} m is not worth surveying"),
            RoadError.NoLampsOnThisGrade =>
                Outcome.No($"{t.Grades[c.A].Name} does not carry street lighting"),
            RoadError.NeedsABridge =>
                Outcome.No("that run crosses open water, and a road is not a bridge"),
            _ => Outcome.No("that run cannot be ordered"),
        };
    }

    /// <summary>
    /// Order a span. It is a site with a bill of materials until the crew and the
    /// steel reach it, and it carries nothing until then.
    /// </summary>
    /// <remarks>
    /// The commissioning number rather than the day: a line takes its turn in the
    /// build queue like anything else, rather than jumping it or waiting behind
    /// every factory in the republic.
    /// </remarks>
    private static Outcome OrderLine(World world, Command c)
    {
        var refusal = world.LineWorks.Order(
            c.A, c.X, c.Y, c.ToX, c.ToY, world.Buildings.Commissioned, out var site);

        return refusal switch
        {
            LineError.None => Outcome.Ok(site!.Id),
            LineError.TooShort =>
                Outcome.No($"a span shorter than {world.Tables.MinLine:0} m is not worth surveying"),
            _ => Outcome.No("that span cannot be ordered"),
        };
    }

    private static Outcome Demolish(World world, Command c)
    {
        var i = world.Buildings.IndexOf(c.A);
        if (i < 0)
        {
            return Outcome.No("there is no such building");
        }

        // Two refusals, and both exist to make an orphaned crew unrepresentable
        // rather than to detect one afterwards: pulling down a site with a gang
        // on it, or an office whose gangs are out, would leave people belonging
        // to a building that no longer exists — and no amount of tidying up
        // afterwards answers "so where are they now?".
        if (world.Crews.WorkingAt(Destination.Building(c.A)) is not null)
        {
            return Outcome.No("there is a crew on that site");
        }

        if (world.Crews.Posted(c.A) > 0)
        {
            return Outcome.No("that office has crews out");
        }

        world.Buildings.Demolish(c.A);
        world.Grid.Detach(c.A);
        world.BuildPolicy.Forget(Destination.Building(c.A));
        return Outcome.Ok();
    }

    private static Outcome SetShifts(World world, Command c)
    {
        var i = world.Buildings.IndexOf(c.A);
        if (i < 0)
        {
            return Outcome.No("there is no such building");
        }

        if (world.Tables.BWorkers[world.Buildings.KindAt(i)] == 0)
        {
            return Outcome.No("nobody works there");
        }

        if (c.B < 0 || c.B > world.Tables.MaxShifts)
        {
            return Outcome.No($"a workplace runs between none and {world.Tables.MaxShifts} crews");
        }

        world.Buildings.SetShiftCount(c.A, c.B);
        return Outcome.Ok();
    }

    private static Outcome SetPriority(World world, Command c)
    {
        if (world.Buildings.IndexOf(c.A) < 0)
        {
            return Outcome.No("there is no such building");
        }

        world.Buildings.SetPriority(c.A, (Priority)c.B);
        return Outcome.Ok();
    }

    private static Outcome SetNationalHours(World world, Command c)
    {
        var t = world.Tables;
        if (double.IsNaN(c.Amount) || c.Amount < t.MinHours || c.Amount > t.MaxHours)
        {
            return Outcome.No(
                $"a working day is between {t.MinHours:0.#} and {t.MaxHours:0.#} hours");
        }

        world.Buildings.SetNationalHours(c.Amount);
        return Outcome.Ok();
    }

    private static Outcome TakeLoan(World world, Command c)
    {
        var market = (Market)c.A;
        var refusal = world.Loans.Take(market, c.B, world.Clock.DayIndex, out var loan);

        if (refusal != LoanError.None || loan is null)
        {
            return Outcome.No(refusal switch
            {
                LoanError.NoSuchTier => "no advance of that size is offered",
                LoanError.AlreadyOwing => "this bloc has already advanced what you have not repaid",
                LoanError.Defaulted => "this bloc has been defaulted on and will not advance again",
                _ => "that advance cannot be taken",
            });
        }

        world.Treasury.Add(market, loan.Principal);
        return Outcome.Ok();
    }

    private static Outcome RepayLoan(World world, Command c)
    {
        var market = (Market)c.A;
        var refusal = world.Loans.Repay(market, c.Amount, world.Treasury, out _);

        return refusal switch
        {
            LoanError.None => Outcome.Ok(),
            LoanError.NothingOwed => Outcome.No("nothing is owed to this bloc"),
            LoanError.CannotAfford => Outcome.No("the treasury cannot cover that"),
            _ => Outcome.No("that repayment cannot be made"),
        };
    }

    private static Outcome AcceptContract(World world, Command c) =>
        world.Contracts.Accept(c.A) ? Outcome.Ok() : Outcome.No("that tender is not on the table");

    private static Outcome DeclineContract(World world, Command c) =>
        world.Contracts.Decline(c.A) ? Outcome.Ok() : Outcome.No("that tender is not on the table");

    /// <summary>
    /// Name the republic — the second beat of founding.
    /// </summary>
    /// <remarks>
    /// A command rather than a field on the spec, because a name is an
    /// <b>input</b>: something a person decided, so it belongs in the journal
    /// beside every other decision. That is what makes a replayed republic come
    /// back called what the player called it rather than nothing.
    /// </remarks>
    private static Outcome NameRepublic(World world, Command c)
    {
        var name = c.Text.Trim();
        if (name.Length == 0)
        {
            return Outcome.No("a republic needs a name");
        }

        if (name.Length > 48)
        {
            return Outcome.No("that name is too long for a letterhead");
        }

        world.Name = name;
        return Outcome.Ok();
    }
}
