namespace RedRepublic.Sim;

/// <summary>
/// The reference player: what a competent republic-builder buys, in the order
/// they would buy it.
/// </summary>
/// <remarks>
/// <para>
/// <b>It lives in the simulation rather than inside the trajectory runner
/// because two things need it and a second copy would be a second answer.</b>
/// The runner reads it to show what an opening looks like; a test asserts
/// against it so the gate can tell a republic that works from one that does not.
/// </para>
/// <para>
/// It touches the simulation only through <see cref="World.Issue"/> and reads,
/// which is what makes it a player rather than a fixture. Nothing here may reach
/// into state directly; if it needs something it cannot ask for, that is a gap in
/// the player's surface and belongs there.
/// </para>
/// <para>
/// It is <b>deliberately dumb</b>: one thing at a time, in a fixed order, paid
/// for however it can be. A cleverer director would be a better player and a
/// worse instrument, because what this is for is finding out whether the opening
/// is <i>possible</i> rather than how well it can be played. <b>A failure under
/// it is therefore not proof the game is unwinnable</b> — but a republic that
/// cuts no coal, moves no freight and feeds nobody for a decade under a sensible
/// fixed order is not a hard game, it is a broken one, and telling those two
/// apart is the whole reason this is reachable from a test.
/// </para>
/// </remarks>
public sealed class Director
{
    /// <summary>
    /// The order a republic gets built in, and every entry is load-bearing.
    /// </summary>
    /// <remarks>
    /// A <b>Construction Office</b> first, because everything after it is
    /// cheaper once the republic has crews of its own. A <b>Motor Depot</b>
    /// second, because a republic without lorries starves beside its own full
    /// bins. Then somewhere to live, and <b>current before the pit</b> — a mine
    /// draws current and cuts nothing without it, where a plant runs on the
    /// first tonne of coal that comes over the border. Building them the other
    /// way round staffs the mine first, because inside a priority band the
    /// labour plan runs in commissioning order, and leaves the republic dark
    /// beside a fully-manned pit it cannot work. Then a <b>Customs House</b>,
    /// which is the only way a rouble ever comes back in. The materials chain comes early
    /// for a reason found the hard way: until the republic can make gravel, brick
    /// and planks, a site it builds itself has nothing to be built out of.
    /// </remarks>
    private static readonly string[] Plan =
    [
        "ConstructionOffice",
        "MotorDepot",
        "Apartment",
        "PowerPlant",
        "TransformerStation",
        "CoalMine",
        "Customs",
        "GravelQuarry",
        "Woodcutter",
        "Sawmill",
        "Brickworks",
        "Store",
        "Farm",
        "FoodFactory",
        "HeatingPlant",
        "BusDepot",
        "Apartment",
        "Clinic",
        "School",
    ];

    /// <summary>
    /// What the republic has to be able to make before its own crews are worth
    /// using.
    /// </summary>
    /// <remarks>
    /// <b>Contractors bring their own materials; your crews do not.</b> A
    /// republic that owns no quarry, no sawmill and no brickworks and switches to
    /// its own crews puts down foundations nothing will ever deliver to — three
    /// buildings in three years against eleven when it kept paying.
    /// </remarks>
    private static readonly string[] Materials = ["GravelQuarry", "Sawmill", "Brickworks"];

    /// <summary>
    /// What the republic buys in until it can make it.
    /// </summary>
    /// <remarks>
    /// <b>The opening is an import problem.</b> A coal mine draws current, a
    /// power plant burns coal to make it, and a republic founded on empty ground
    /// has neither — so the circle cannot be broken from the inside and the first
    /// tonne has to come over the border. Fuel and machinery are the same shape.
    /// Steel and brick are here because a span the republic orders and never
    /// receives steel for is a grid that stays an order for ever.
    /// </remarks>
    private static readonly string[] Imports =
        ["Food", "Coal", "Fuel", "Machinery", "Steel", "Bricks"];

    /// <summary>What is staffed before anything else, in a republic short of hands.</summary>
    private static readonly string[] LifeSupport =
    [
        "PowerPlant", "TransformerStation", "CoalMine", "HeatingPlant",
        "Farm", "FoodFactory", "Store", "Customs",
    ];

    /// <summary>And what is staffed next: nothing reaches anything without it.</summary>
    private static readonly string[] Haulage =
        ["MotorDepot", "ConstructionOffice", "BusDepot"];

    private readonly double _centreX;
    private readonly double _centreY;
    private readonly List<string> _said = [];

    /// <summary>
    /// Spans that were refused and are not worth asking for again.
    /// </summary>
    /// <remarks>
    /// The same escape <see cref="BuildNext"/> has, and it is here for the same
    /// reason: without it one consumer the grid cannot reach is asked for every
    /// month for a decade, the network never grows past it, and every column
    /// after it prints a flat line that reads as balance.
    /// </remarks>
    private readonly Dictionary<(int Kind, int Building), int> _refused = [];

    /// <summary>Buildings whose standing has been set, so it is set once.</summary>
    private readonly HashSet<int> _ranked = [];

    private int _step;
    private bool _hired;
    private bool _selling;
    private bool _buying;
    private bool _hauling;
    private int _stuck;

    public Director(double centreX, double centreY)
    {
        _centreX = centreX;
        _centreY = centreY;
    }

    /// <summary>What it has said out loud, in the order it said it.</summary>
    /// <remarks>
    /// Each line once, so a decade does not print the same sentence three
    /// hundred times.
    /// </remarks>
    public IReadOnlyList<string> Said => _said;

    /// <summary>Lines said since this was last called, and then cleared.</summary>
    public List<string> Drain()
    {
        var fresh = new List<string>(_fresh);
        _fresh.Clear();
        return fresh;
    }

    private readonly List<string> _fresh = [];

    /// <summary>One month of decisions.</summary>
    public void Month(World world)
    {
        ArgumentNullException.ThrowIfNull(world);
        BuildNext(world);
        Roster(world);
        Hire(world);
        Trade(world);
        Grid(world);
        Haul(world);
    }

    private void Say(string line)
    {
        if (!_said.Contains(line))
        {
            _said.Add(line);
            _fresh.Add(line);
        }
    }

    /// <summary>
    /// Put the next thing on the plan up, if there is room in the queue.
    /// </summary>
    /// <remarks>
    /// <b>Two on the go, not one.</b> Waiting for every site to finish means a
    /// single site that can never finish — one the republic ordered itself with
    /// no materials to build it from — stops the plan dead for the rest of the
    /// run, and every column after it prints a flat line that reads as balance.
    /// </remarks>
    private void BuildNext(World world)
    {
        if (_step >= Plan.Length)
        {
            return;
        }

        var going = 0;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b))
            {
                going++;
            }
        }

        if (going >= 2)
        {
            return;
        }

        var t = world.Tables;
        var kind = t.BuildingIndex(Plan[_step]);
        var aroundX = _centreX;
        var aroundY = _centreY;

        // <b>A customs house goes to the border, not to the town</b> — the one
        // building that has to stand somewhere the republic did not choose. And
        // it goes to an <b>Eastern</b> post, because the grant is roubles: a
        // house at a Western crossing is one a purse holding no dollars can buy
        // nothing through, and trade then fails silently in both directions.
        if (Plan[_step] == "Customs")
        {
            var post = world.Frontier.NearestCrossing(_centreX, _centreY, Market.East);
            if (post is null)
            {
                Say("no Eastern post in reach — this posting must earn dollars first");
                post = world.Frontier.NearestCrossing(_centreX, _centreY, null);
            }

            if (post is null)
            {
                Say("this republic has no frontier post at all");
                _step++;
                return;
            }

            aroundX = post.Value.X;
            aroundY = post.Value.Y;
        }

        var at = Scenario.FindSite(world, kind, aroundX, aroundY, 1_400.0);
        if (at is null)
        {
            Say($"nowhere within reach takes a {t.BName[kind]}");
            _step++;
            return;
        }

        // Build it with your own crews if there is an office with people in it
        // and the republic can make what a site is made of, and pay a Bloc firm
        // otherwise. That is the whole shape of the opening and the reason
        // contractors exist at all.
        var own = Staffed(world, "ConstructionOffice");
        foreach (var want in Materials)
        {
            own &= Staffed(world, want);
        }

        var outcome = own
            ? world.Issue(Command.Place(kind, at.Value.X, at.Value.Y))
            : world.Issue(Command.ContractBuild(kind, at.Value.X, at.Value.Y, Market.East));

        if (outcome.Accepted)
        {
            Say($"{(own ? "building" : "contracting")} a {t.BName[kind]}");
            _step++;
            _stuck = 0;
            return;
        }

        // <b>Give up on it rather than trying for ever.</b> Staying on a refused
        // step means one building the director cannot site stops the plan dead,
        // and every column after it prints a flat line that reads as balance. A
        // player would move on; so does this.
        Say($"cannot start a {t.BName[kind]}: {outcome.Refusal}");
        if (++_stuck >= 3)
        {
            _stuck = 0;
            _step++;
        }
    }

    /// <summary>
    /// Say which enterprise gets the last worker.
    /// </summary>
    /// <remarks>
    /// <para>
    /// <b>The central decision of a planned economy, and the plan makes it by
    /// accident otherwise.</b> Every life-support kind is authored
    /// <c>First</c>, and inside a band the labour plan runs in commissioning
    /// order — so whichever the director happened to build second takes the
    /// people, and a republic of twenty-four put sixteen of them in a lorry
    /// yard and left the power station with none. Every column after that reads
    /// as balance.
    /// </para>
    /// <para>
    /// The ranking below is fixed and dull, like everything else here: current
    /// and food before haulage, haulage before anything else. A dark republic
    /// produces nothing at all; one with its lorries parked merely waits.
    /// </para>
    /// </remarks>
    private void Roster(World world)
    {
        var t = world.Tables;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            var id = world.Buildings.IdAt(b);
            if (!world.Buildings.IsBuilt(b) || !_ranked.Add(id))
            {
                continue;
            }

            var name = t.BuildingIds[world.Buildings.KindAt(b)];
            var standing = Array.IndexOf(LifeSupport, name) >= 0
                ? Priority.First
                : Array.IndexOf(Haulage, name) >= 0
                    ? Priority.Ordinary
                    : Priority.Last;

            world.Issue(Command.SetPriority(id, standing));
        }
    }

    /// <summary>
    /// Buy builders from the Eastern Bloc once there is an office to put them
    /// in. A blank map has nobody at all, so the first crews are always bought.
    /// </summary>
    private void Hire(World world)
    {
        if (_hired)
        {
            return;
        }

        var office = Find(world, "ConstructionOffice");
        if (office < 0)
        {
            return;
        }

        var outcome = world.Issue(
            Command.HireForeign(Market.East, world.Buildings.IdAt(office), 20));

        if (outcome.Accepted)
        {
            _hired = true;
            Say("hired twenty builders from the Eastern Bloc");
        }
        else
        {
            Say($"cannot hire abroad: {outcome.Refusal}");
        }
    }

    /// <summary>
    /// Sell coal, and buy what the republic cannot yet make.
    /// </summary>
    /// <remarks>
    /// <b>Not a fixed market.</b> A customs house clears only for the bloc whose
    /// post it stands at, and which post is nearest is decided by the land.
    /// Selling east from a Western crossing earns nothing at all.
    /// </remarks>
    private void Trade(World world)
    {
        var house = Find(world, "Customs");
        if (house < 0)
        {
            return;
        }

        var t = world.Tables;
        var bloc = world.Frontier.BlocNear(
            world.Buildings.XAt(house), world.Buildings.YAt(house));

        if (!_buying)
        {
            var all = true;
            foreach (var id in Imports)
            {
                all &= world.Issue(
                    Command.AddTradeRule(t.ResourceIndex(id), bloc, TradeAction.Buy)).Accepted;
            }

            if (all)
            {
                _buying = true;
                Say($"buying coal, fuel and machinery from the {bloc}");
            }
        }

        // Coal goes on sale only once the republic actually digs it. Selling
        // from the day the house opens sells the imports back at a loss.
        if (!_selling && Staffed(world, "CoalMine"))
        {
            if (world.Issue(
                Command.AddTradeRule(t.ResourceIndex("Coal"), bloc, TradeAction.Sell)).Accepted)
            {
                _selling = true;
                Say($"selling coal to the {bloc}");
            }
        }
    }

    /// <summary>
    /// String the power line, and then keep stringing it.
    /// </summary>
    /// <remarks>
    /// <b>A plant that is not wired to anything lights nothing, including
    /// itself.</b> A republic that has bought a power station, a transformer and
    /// a coal mine and strung no span sits dark for ever, and the mine never cuts
    /// a tonne because a mine draws current. One span under construction at a
    /// time across both networks, because a republic with one office and one gang
    /// cannot work two — and current before heat, because a dark republic
    /// produces nothing at all where a cold one merely suffers.
    /// </remarks>
    private void Grid(World world)
    {
        if (world.LineWorks.Sites.Count > 0)
        {
            return;
        }

        if (!StringOut(world, "Power"))
        {
            StringOut(world, "Heat");
        }
    }

    /// <summary>
    /// A way to the border, once there is something to drive on it.
    /// </summary>
    /// <remarks>
    /// <b>The first long haul a republic makes is to its own customs house</b>,
    /// and across country a lorry crawls. Nothing else in the plan produces a
    /// metre of road, so without this the instrument measures a decade of a
    /// republic that never built one — in a game whose bar is a road-logistics
    /// game. A dirt track, because it costs nothing but builder-days and the
    /// opening has no gravel.
    /// </remarks>
    private void Haul(World world)
    {
        if (_hauling || world.RoadWorks.Sites.Count > 0)
        {
            return;
        }

        var house = Find(world, "Customs");
        if (house < 0)
        {
            return;
        }

        var run = world.Issue(Command.OrderRoad(
            _centreX, _centreY,
            world.Buildings.XAt(house), world.Buildings.YAt(house),
            world.Tables.GradeIndex("Dirt"), false));

        if (run.Accepted)
        {
            _hauling = true;
            Say("laying a track out to the customs house");
        }
        else
        {
            _hauling = true; // a run this one cannot survey is not one to re-survey
            Say($"no track to the customs house: {run.Refusal}");
        }
    }

    /// <summary>
    /// Order one span of one network, and say whether anything was ordered.
    /// </summary>
    /// <remarks>
    /// Source to the hub first, then the hub outward to whichever consumer is
    /// nearest and not yet joined — so the grid grows out of the town rather than
    /// leaping across the map. Written once for both networks because a heat main
    /// is the same problem as a power line in every respect that matters here.
    /// </remarks>
    private bool StringOut(World world, string utility)
    {
        var t = world.Tables;
        var kind = t.UtilityIndex(utility);
        var heat = utility == "Heat";

        var source = -1;
        for (var b = 0; b < world.Buildings.Count && source < 0; b++)
        {
            var k = world.Buildings.KindAt(b);
            var makes = heat ? t.BHeatOutput[k] > 0.0 : t.BPowerOutput[k] > 0.0;
            if (world.Buildings.IsBuilt(b) && makes)
            {
                source = b;
            }
        }

        if (source < 0)
        {
            return false; // nothing to carry until something makes it
        }

        // The source to the hub first: until that span exists there is no
        // network for anything else to join.
        if (world.Grid.NetworkOf(world.Buildings.IdAt(source), kind) < 0)
        {
            if (GaveUpOn(kind, world.Buildings.IdAt(source)))
            {
                return false;
            }

            var laid = world.Issue(Command.OrderLine(
                kind, world.Buildings.XAt(source), world.Buildings.YAt(source), _centreX, _centreY));

            Say(laid.Accepted
                ? $"laying the {t.Utilities[kind].Name} in from the works"
                : $"no {t.Utilities[kind].Name} from the works: {laid.Refusal}");
            return Refused(kind, world.Buildings.IdAt(source), laid.Accepted);
        }

        // Then outward, nearest first.
        var nearest = -1;
        var gap = double.PositiveInfinity;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            var k = world.Buildings.KindAt(b);
            var wants = heat ? t.BHeat[k] > 0.0 : t.BPowerDraw[k] > 0.0;
            if (!world.Buildings.IsBuilt(b) || !wants
                || world.Grid.NetworkOf(world.Buildings.IdAt(b), kind) >= 0
                || GaveUpOn(kind, world.Buildings.IdAt(b)))
            {
                continue;
            }

            var distance = Units.Distance(
                _centreX, _centreY, world.Buildings.XAt(b), world.Buildings.YAt(b));
            if (distance < gap)
            {
                gap = distance;
                nearest = b;
            }
        }

        if (nearest < 0)
        {
            return false;
        }

        var name = t.BName[world.Buildings.KindAt(nearest)];
        var run = world.Issue(Command.OrderLine(
            kind, _centreX, _centreY,
            world.Buildings.XAt(nearest), world.Buildings.YAt(nearest)));

        Say(run.Accepted
            ? $"running the {t.Utilities[kind].Name} out to the {name}"
            : $"no {t.Utilities[kind].Name} to the {name}: {run.Refusal}");
        return Refused(kind, world.Buildings.IdAt(nearest), run.Accepted);
    }

    /// <summary>
    /// Record how a span went, and answer whether anything was actually ordered.
    /// </summary>
    /// <remarks>
    /// <b>The answer is "did I order one", not "did I try".</b> Returning true
    /// on a refusal is what wedged this: <see cref="Grid"/> read it as a span
    /// under way, never fell through to the heat main, and asked the same
    /// refused consumer again every month for nine years — with the
    /// once-only <see cref="Say"/> hiding the repetition.
    /// </remarks>
    private bool Refused(int kind, int building, bool accepted)
    {
        if (accepted)
        {
            _refused.Remove((kind, building));
            return true;
        }

        _refused[(kind, building)] = _refused.GetValueOrDefault((kind, building)) + 1;
        return false;
    }

    private bool GaveUpOn(int kind, int building) =>
        _refused.GetValueOrDefault((kind, building)) >= 3;

    /// <summary>The first finished building of a kind, or -1.</summary>
    private static int Find(World world, string id)
    {
        var kind = world.Tables.BuildingIndex(id);
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (world.Buildings.IsBuilt(b) && world.Buildings.KindAt(b) == kind)
            {
                return b;
            }
        }

        return -1;
    }

    /// <summary>Whether a kind stands finished somewhere with people in it.</summary>
    private static bool Staffed(World world, string id)
    {
        var kind = world.Tables.BuildingIndex(id);
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (world.Buildings.IsBuilt(b) && world.Buildings.KindAt(b) == kind
                && world.Buildings.StaffAt(b) > 0)
            {
                return true;
            }
        }

        return false;
    }
}
