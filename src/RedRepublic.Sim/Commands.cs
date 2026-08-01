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

    /// <summary>
    /// Call off a way or a span that has been ordered but not finished.
    /// </summary>
    /// <remarks>
    /// <b>A misclick is otherwise permanent.</b> A run ordered across the map
    /// draws a gang and every lorry the republic has until it is done, and
    /// there was no way to stop it — which is a thing the game this one is
    /// measured against has had since its first release. What has already been
    /// delivered to the site is lost with it, exactly as the contents of a
    /// demolished building are: the gravel is on the ground out there.
    /// </remarks>
    CancelWorks,
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

    /// <summary>Call off a way or a span that is under construction.</summary>
    public static Command CancelWorks(Destination site) =>
        new(CommandKind.CancelWorks, (int)site.Kind, site.Id);

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

    /// <summary>Call a building crew off a site.</summary>
    public static Command RecallCrew(Destination site) =>
        new(CommandKind.RecallCrew, (int)site.Kind, site.Id);

    /// <summary>
    /// Say where a site buys materials the republic has not made, or where every
    /// site does.
    /// </summary>
    /// <remarks>
    /// <paramref name="site"/> is <c>null</c> for the republic's default, and
    /// <paramref name="crossing"/> is <c>null</c> to import nothing. The two mean
    /// different things, which is why neither is a bare id: a site set to nothing
    /// while the default is set has been opted <i>out</i>, and a site with no
    /// instruction at all follows the default.
    /// </remarks>
    public static Command SetImportPolicy(Destination? site, int? crossing) =>
        new(CommandKind.SetImportPolicy,
            site is null ? -1 : (int)site.Value.Kind,
            site?.Id ?? -1,
            crossing ?? -1,
            Flag: site is not null);

    /// <summary>Put a site back under the republic's default policy.</summary>
    public static Command ClearImportPolicy(Destination site) =>
        new(CommandKind.ClearImportPolicy, (int)site.Kind, site.Id);

    /// <summary>
    /// Tell a terminal or a distribution office what to keep on hand. Zero
    /// cancels the order.
    /// </summary>
    public static Command SetStandingOrder(int building, int resource, double tonnes) =>
        new(CommandKind.SetStandingOrder, building, resource, Amount: tonnes);

    /// <summary>Hire builders from a bloc, for one Construction Office.</summary>
    public static Command HireForeign(Market market, int office, int heads) =>
        new(CommandKind.HireForeign, (int)market, office, heads);

    public static Command AddTradeRule(int resource, Market market, TradeAction action) =>
        new(CommandKind.AddTradeRule, resource, (int)market, (int)action);

    public static Command RemoveTradeRule(int index) =>
        new(CommandKind.RemoveTradeRule, index);

    public static Command MoveTradeRule(int from, int to) =>
        new(CommandKind.MoveTradeRule, from, to);

    /// <summary>
    /// Set — or with a null <paramref name="hours"/> clear — an exception to the
    /// national working day, for every building of a kind.
    /// </summary>
    /// <remarks>
    /// A rule about a kind covers every such building including ones not yet
    /// built; a rule about one building beats it. Clearing falls back to the rule
    /// above rather than freezing whatever number was in force.
    /// </remarks>
    public static Command SetKindHours(int kind, double? hours) =>
        new(CommandKind.SetShiftHours, kind, Amount: hours ?? 0.0, Flag: hours is not null);

    public static Command SetBuildingHours(int building, double? hours) =>
        new(CommandKind.SetShiftHours, -1, building,
            Amount: hours ?? 0.0, Flag: hours is not null);
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
            CommandKind.CancelWorks => CancelWorks(world, command),
            CommandKind.SetShifts => SetShifts(world, command),
            CommandKind.SetPriority => SetPriority(world, command),
            CommandKind.SetNationalShiftHours => SetNationalHours(world, command),
            CommandKind.TakeLoan => TakeLoan(world, command),
            CommandKind.RepayLoan => RepayLoan(world, command),
            CommandKind.AcceptContract => AcceptContract(world, command),
            CommandKind.DeclineContract => DeclineContract(world, command),
            CommandKind.OrderRoad => OrderRoad(world, command),
            CommandKind.OrderLine => OrderLine(world, command),
            CommandKind.RecallCrew => RecallCrew(world, command),
            CommandKind.SetImportPolicy => SetImportPolicy(world, command),
            CommandKind.ClearImportPolicy => ClearImportPolicy(world, command),
            CommandKind.SetStandingOrder => SetStandingOrder(world, command),
            CommandKind.HireForeign => HireForeign(world, command),
            CommandKind.AddTradeRule or CommandKind.RemoveTradeRule
                or CommandKind.MoveTradeRule => EditRules(world, command),
            CommandKind.SetShiftHours => SetShiftHours(world, command),
            CommandKind.NameRepublic => NameRepublic(world, command),
            _ => Outcome.No("that is not something the republic can do yet"),
        };
    }

    /// <summary>
    /// Call the gang off a site. They down tools where they stand and wait for
    /// the office to send a bus.
    /// </summary>
    /// <remarks>
    /// There is no way to make people appear back at the office — the same rule
    /// that makes construction physical in the first place. It exists because a
    /// site whose materials never arrive would otherwise hold its gang for ever,
    /// and because the alternative — letting the simulation decide when a crew
    /// has waited long enough — is a judgement about the player's plan that the
    /// player should be making.
    /// </remarks>
    private static Outcome RecallCrew(World world, Command c)
    {
        var site = new Destination((DestinationKind)c.A, c.B, 0.0, 0.0);
        var at = Systems.Where(world, site);
        if (at is null)
        {
            return Outcome.No("there is no such site");
        }

        return world.Crews.Release(site, at.Value.X, at.Value.Y)
            ? Outcome.Ok()
            : Outcome.No("nobody is working that site");
    }

    /// <summary>
    /// Say where sites buy materials the republic has not made.
    /// </summary>
    /// <remarks>
    /// The crossing is checked here rather than when it is read, so a policy in
    /// the world always names a post that exists. A typo would otherwise sit in a
    /// save quietly importing nothing, which looks exactly like a republic that
    /// cannot afford anything.
    /// <b>This shortens no build.</b> It answers where a tonne of brick comes
    /// from in a republic with no brickworks; the goods land at the post and your
    /// lorries still have to fetch them.
    /// </remarks>
    private static Outcome SetImportPolicy(World world, Command c)
    {
        if (c.C >= 0 && world.Frontier.Get(c.C) is null)
        {
            return Outcome.No($"there is no frontier post {c.C}");
        }

        var crossing = c.C < 0 ? (int?)null : c.C;
        if (c.Flag)
        {
            world.BuildPolicy.SetSite(new Destination((DestinationKind)c.A, c.B, 0.0, 0.0), crossing);
        }
        else
        {
            world.BuildPolicy.SetGlobal(crossing);
        }

        return Outcome.Ok();
    }

    private static Outcome ClearImportPolicy(World world, Command c) =>
        world.BuildPolicy.ClearSite(new Destination((DestinationKind)c.A, c.B, 0.0, 0.0))
            ? Outcome.Ok()
            : Outcome.No("this site already follows the republic's import policy");

    /// <summary>
    /// Tell a terminal or a distribution office what to keep on hand.
    /// </summary>
    /// <remarks>
    /// <b>The command that makes a station worth building.</b> Nothing in the
    /// republic wants to deliver to one otherwise: a station consumes nothing and
    /// sells nothing, so the freight ranking has no reason to look at it. A
    /// standing order gives it a demand of its own and the ordinary ranking does
    /// the rest.
    /// </remarks>
    private static Outcome SetStandingOrder(World world, Command c)
    {
        var t = world.Tables;
        if (c.B < 0 || c.B >= t.Resources.Length)
        {
            return Outcome.No("the republic does not trade in that");
        }

        var i = world.Buildings.IndexOf(c.A);
        if (i < 0)
        {
            return Outcome.No("there is no such building");
        }

        var kind = world.Buildings.KindAt(i);
        if (!t.BStoresToOrder[kind])
        {
            return Outcome.No(
                "only a terminal, a store or a distribution office keeps goods to order");
        }

        // A tank is not a coal bunker, and this is where the player is told so.
        // Letting the order stand and quietly never fill is the whole difference
        // between a rule and a mystery: the intake capacity would be zero for
        // ever and the panel would show an order nothing was ever sent for.
        if (!world.Buildings.Takes(i, c.B))
        {
            return Outcome.No($"{t.ResourceNames[c.B]} will not go in a {t.BName[kind]}");
        }

        var holds = world.Buildings.StorageCap(i);
        if (c.Amount > holds)
        {
            return Outcome.No($"asked for {c.Amount:0} t where only {holds:0} t will fit");
        }

        world.Buildings.Orders.Set(i, c.B, c.Amount);
        return Outcome.Ok();
    }

    /// <summary>
    /// Hire builders from a bloc, for one Construction Office.
    /// </summary>
    /// <remarks>
    /// One transaction at a genuine boundary: the fee leaves and the workers set
    /// out, and neither happens without the other. <b>They arrive at a frontier
    /// post, not in the yard</b> — a bus has to go and fetch them, over the roads
    /// the republic has built, exactly as a crew coming off a finished site does.
    /// </remarks>
    private static Outcome HireForeign(World world, Command c)
    {
        var t = world.Tables;
        if (!Enum.IsDefined((Market)c.A))
        {
            return Outcome.No("there is no such bloc to hire from");
        }

        var market = (Market)c.A;
        if (c.C <= 0)
        {
            return Outcome.No("no workers were asked for");
        }

        var i = world.Buildings.IndexOf(c.B);
        if (i < 0)
        {
            return Outcome.No("there is no such building");
        }

        if (t.BuildingIds[world.Buildings.KindAt(i)] != "ConstructionOffice"
            || !world.Buildings.IsBuilt(i))
        {
            return Outcome.No("builders are hired to a Construction Office");
        }

        // They arrive at their own bloc's post, which is what makes hiring from
        // the far bloc a haulage decision rather than a dropdown — the same
        // geography a dollar already has.
        var post = world.Frontier.NearestCrossing(
            world.Buildings.XAt(i), world.Buildings.YAt(i), market);
        if (post is null)
        {
            return Outcome.No(
                $"the {market} holds no frontier post here, so its workers have nowhere to arrive");
        }

        var fee = c.C * t.HiringFee;
        var held = world.Treasury.Of(market);
        if (held + 1e-9 < fee)
        {
            return Outcome.No(
                $"placing them costs {fee:0} and the treasury holds {held:0}");
        }

        world.Treasury.Take(market, fee);
        world.Crews.Hire(c.B, market, c.C);

        // A gang standing at the post, not a party of settlers. They are the
        // office's hands from the moment they land — what they need is a bus,
        // not a bed, and the difference is why this is a crew rather than an
        // arrival.
        world.Crews.Send(c.B, c.C, post.Value.X, post.Value.Y, market);
        return Outcome.Ok();
    }

    /// <summary>
    /// Edit the standing instructions to the customs houses.
    /// </summary>
    /// <remarks>
    /// Moving a rule is its own command rather than a re-send of the whole
    /// policy, because <b>the order is the decision</b>: when throughput or money
    /// runs short the first rule is served first, and that ranking is the
    /// player's to make.
    /// </remarks>
    private static Outcome EditRules(World world, Command c)
    {
        var rules = world.TradeRules;

        if (c.Kind == CommandKind.AddTradeRule)
        {
            if (c.A < 0 || c.A >= world.Tables.Resources.Length)
            {
                return Outcome.No("the republic does not trade in that");
            }

            if (!Enum.IsDefined((Market)c.B) || !Enum.IsDefined((TradeAction)c.C))
            {
                return Outcome.No("that is not an instruction a customs house takes");
            }

            rules.Add(new TradeRule(c.A, (Market)c.B, (TradeAction)c.C));
            return Outcome.Ok();
        }

        if (c.A < 0 || c.A >= rules.Count)
        {
            return Outcome.No(NoSuchRule(c.A, rules.Count));
        }

        if (c.Kind == CommandKind.RemoveTradeRule)
        {
            rules.RemoveAt(c.A);
            return Outcome.Ok();
        }

        if (c.B < 0 || c.B >= rules.Count)
        {
            return Outcome.No(NoSuchRule(c.B, rules.Count));
        }

        var rule = rules[c.A];
        rules.RemoveAt(c.A);
        rules.Insert(c.B, rule);
        return Outcome.Ok();

        static string NoSuchRule(int index, int count) => count switch
        {
            0 => "there are no trade rules to change",
            1 => $"there is only one trade rule, and it is not {index}",
            _ => $"there is no trade rule {index}; there are {count}",
        };
    }

    /// <summary>
    /// Set or clear an exception to the national working day.
    /// </summary>
    /// <remarks>
    /// <i>"Doctors work twelve, but at this hospital fourteen."</i> A rule about
    /// a kind covers every such building including ones not yet built; a rule
    /// about one building beats it. Clearing falls back to the rule above rather
    /// than freezing whatever number was in force.
    /// </remarks>
    private static Outcome SetShiftHours(World world, Command c)
    {
        var t = world.Tables;
        double? hours = c.Flag ? c.Amount : null;

        if (hours is not null
            && (double.IsNaN(hours.Value) || hours < t.MinHours || hours > t.MaxHours))
        {
            return Outcome.No(
                $"a shift of {hours:0} hours is not one the republic will roster; "
                + $"between {t.MinHours:0} and {t.MaxHours:0}");
        }

        if (c.A >= 0)
        {
            if (c.A >= t.BuildingIds.Length)
            {
                return Outcome.No("the republic does not build that");
            }

            if (hours is null && world.Buildings.Shifts.OfKind(c.A) is null)
            {
                return Outcome.No("those buildings keep the standard working day already");
            }

            world.Buildings.SetKindHours(c.A, hours);
            return Outcome.Ok();
        }

        if (world.Buildings.IndexOf(c.B) < 0)
        {
            return Outcome.No("there is no such building");
        }

        if (hours is null && world.Buildings.Shifts.OfBuilding(c.B) is null)
        {
            return Outcome.No("that building keeps the standard working day already");
        }

        world.Buildings.SetBuildingHours(c.B, hours);
        return Outcome.Ok();
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
        // Which bloc's firm, checked before the ground is: the contractor is read
        // every day of the build to bill a treasury, and a market nothing answers
        // to would bring the republic down on the tick rather than here.
        if (!Enum.IsDefined((Market)c.B))
        {
            return Outcome.No("there is no such bloc to contract with");
        }

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

        // An id nothing answers to is refused rather than indexed. A command
        // arrives from outside the simulation, so a nonsense one is an input to
        // decline and never a reason to bring the republic down.
        if (kind < 0 || kind >= t.BuildingIds.Length)
        {
            return "the republic does not build that";
        }

        if (!world.Terrain.Contains(x, y))
        {
            return "that is outside the republic";
        }

        if (!world.Terrain.AreaIsBuildable(x, y, t.BWidth[kind], t.BDepth[kind]))
        {
            return "the ground there will not take it";
        }

        if (world.Buildings.WouldOverlap(kind, x, y))
        {
            return "something already stands there";
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
        if (c.A < 0 || c.A >= t.Grades.Length)
        {
            return Outcome.No("the republic does not lay that kind of way");
        }

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
        if (c.A < 0 || c.A >= world.Tables.Utilities.Length)
        {
            return Outcome.No("the republic does not string that kind of line");
        }

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

        // Three refusals, and all of them exist to make an orphaned thing
        // unrepresentable rather than to detect one afterwards: pulling down a
        // site with a gang on it, or an office whose gangs are out, would leave
        // people belonging to a building that no longer exists — and no amount
        // of tidying up afterwards answers "so where are they now?".
        if (world.Crews.WorkingAt(Destination.Building(c.A)) is not null)
        {
            return Outcome.No("there is a crew on that site");
        }

        if (world.Crews.Posted(c.A) > 0)
        {
            return Outcome.No("that office has crews out");
        }

        // And the same for what it keeps in its yard. A garage's vehicles
        // belong to it and go with it; one that is out on a job does not, so
        // the yard is pulled down when they are all home. Without this a
        // republic demolished a depot and rebuilt it for a second free fleet,
        // over and over.
        var parked = new List<int>();
        foreach (var v in world.Fleet.OfGarage(c.A))
        {
            if (world.Fleet.StateAt(v) != VehicleState.Idle)
            {
                return Outcome.No("that garage has vehicles out on the road");
            }

            parked.Add(v);
        }

        // Backwards, so removing one does not shift the next out from under us.
        parked.Sort();
        for (var v = parked.Count - 1; v >= 0; v--)
        {
            world.Fleet.RemoveAt(parked[v]);
        }

        world.Buildings.Demolish(c.A);
        world.Grid.Detach(c.A);
        world.BuildPolicy.Forget(Destination.Building(c.A));
        return Outcome.Ok();
    }

    /// <summary>
    /// Call off a way or a span that has been ordered but not finished.
    /// </summary>
    /// <remarks>
    /// The same refusal a demolition makes, and for the same reason: a gang
    /// working a site that stopped existing is people belonging to nowhere.
    /// Call them off first, which is what <see cref="CommandKind.RecallCrew"/>
    /// is for.
    /// </remarks>
    private static Outcome CancelWorks(World world, Command c)
    {
        var site = new Destination((DestinationKind)c.A, c.B, 0.0, 0.0);
        if (world.Crews.WorkingAt(site) is not null)
        {
            return Outcome.No("there is a crew on that site; call them off first");
        }

        switch (site.Kind)
        {
            case DestinationKind.RoadSite:
                {
                    var run = world.RoadWorks.Get(site.Id);
                    if (run is null)
                    {
                        return Outcome.No("there is no such run under construction");
                    }

                    world.RoadWorks.Finish(run);
                    break;
                }

            case DestinationKind.LineSite:
                {
                    var span = world.LineWorks.Get(site.Id);
                    if (span is null)
                    {
                        return Outcome.No("there is no such span under construction");
                    }

                    world.LineWorks.Finish(span);
                    break;
                }

            default:
                return Outcome.No("a building is pulled down rather than called off");
        }

        world.BuildPolicy.Forget(site);
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
        if (!Enum.IsDefined((Market)c.A))
        {
            return Outcome.No("there is no such bloc");
        }

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
        if (!Enum.IsDefined((Market)c.A))
        {
            return Outcome.No("there is no such bloc");
        }

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
