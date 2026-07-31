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
    /// bins. Then somewhere to live, power, and a <b>Customs House</b>, which is
    /// the only way a rouble ever comes back in. The materials chain comes early
    /// for a reason found the hard way: until the republic can make gravel, brick
    /// and planks, a site it builds itself has nothing to be built out of.
    /// </remarks>
    private static readonly string[] Plan =
    [
        "ConstructionOffice",
        "MotorDepot",
        "Apartment",
        "CoalMine",
        "PowerPlant",
        "TransformerStation",
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

    private readonly double _centreX;
    private readonly double _centreY;
    private readonly List<string> _said = [];

    private int _step;
    private bool _hired;
    private bool _selling;
    private bool _buying;
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
        Hire(world);
        Trade(world);
        Grid(world);
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
            var laid = world.Issue(Command.OrderLine(
                kind, world.Buildings.XAt(source), world.Buildings.YAt(source), _centreX, _centreY));

            Say(laid.Accepted
                ? $"laying the {t.Utilities[kind].Name} in from the works"
                : $"no {t.Utilities[kind].Name} from the works: {laid.Refusal}");
            return true;
        }

        // Then outward, nearest first.
        var nearest = -1;
        var gap = double.PositiveInfinity;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            var k = world.Buildings.KindAt(b);
            var wants = heat ? t.BHeat[k] > 0.0 : t.BPowerDraw[k] > 0.0;
            if (!world.Buildings.IsBuilt(b) || !wants
                || world.Grid.NetworkOf(world.Buildings.IdAt(b), kind) >= 0)
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
        return true;
    }

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
