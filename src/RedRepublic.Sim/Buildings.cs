namespace RedRepublic.Sim;

/// <summary>
/// Where a building stands in the labour plan when the republic is short of
/// hands.
/// </summary>
/// <remarks>
/// Ordered so that <c>First</c> is served first. Which enterprise gets the last
/// worker is the central decision of a planned economy; the table holds a
/// sensible opening position for it, and the player disagrees with the table.
/// </remarks>
public enum Priority
{
    Last,
    Ordinary,
    First,
}

/// <summary>Why a building could not be put there.</summary>
public enum PlacementError
{
    Unbuildable,
    Occupied,
    NothingToTap,
    NotOnTheBorder,
    OffTheMap,
    NoNetworkWithinReach,
}

/// <summary>
/// Every building in the republic, held as parallel arrays.
/// </summary>
/// <remarks>
/// <para>
/// One array per field rather than one object per building. The per-tick
/// production pass walks every building every tick and the top speed buys 480
/// ticks a second; measured on this machine, the difference between this shape
/// and an object per building is the difference between clearing that and
/// missing it by half.
/// </para>
/// <para>
/// A building is addressed by its <b>index</b> inside the simulation and by its
/// <b>id</b> everywhere else. Ids are never reused, so a save, a journal entry
/// or a command that names one always means the same building even after
/// something between them was pulled down.
/// </para>
/// </remarks>
public sealed class Buildings
{
    private readonly Tables _t;
    private readonly List<int> _id = [];
    private readonly List<int> _kind = [];
    private readonly List<double> _x = [];
    private readonly List<double> _y = [];
    private readonly List<Priority> _priority = [];
    private readonly List<int> _staff = [];
    private readonly List<bool> _powered = [];
    private readonly List<bool> _heated = [];
    private readonly List<double> _provisioned = [];
    private readonly List<double> _comforted = [];
    private readonly List<double> _drink = [];
    private readonly List<Contentment> _content = [];
    private readonly List<double> _workDone = [];
    private readonly List<int> _contractor = [];
    private readonly List<int> _tapped = [];
    private readonly List<int> _shifts = [];
    private readonly List<double> _hours = [];

    private int _nextId = 1;

    public Buildings(Tables tables)
    {
        ArgumentNullException.ThrowIfNull(tables);
        _t = tables;
        Shifts = new ShiftPolicy(tables);
        Stock = new GrowableStock(tables.Resources.Length);
        Orders = new GrowableStock(tables.Resources.Length);
    }

    /// <summary>How many buildings stand, built or not.</summary>
    public int Count => _id.Count;

    /// <summary>How many have ever been commissioned — ids are never reused.</summary>
    public int Commissioned => _nextId - 1;

    /// <summary>What each building is holding.</summary>
    public GrowableStock Stock { get; }

    /// <summary>
    /// What a terminal or distribution office has been told to keep on hand.
    /// </summary>
    /// <remarks>
    /// The command that makes a station worth building: nothing in the republic
    /// wants to deliver to one otherwise, because a station consumes nothing and
    /// sells nothing. A standing order gives it a demand of its own.
    /// </remarks>
    public GrowableStock Orders { get; }

    public ShiftPolicy Shifts { get; }

    public int IdAt(int i) => _id[i];

    public int KindAt(int i) => _kind[i];

    public double XAt(int i) => _x[i];

    public double YAt(int i) => _y[i];

    public Priority PriorityAt(int i) => _priority[i];

    public int StaffAt(int i) => _staff[i];

    public bool PoweredAt(int i) => _powered[i];

    public bool HeatedAt(int i) => _heated[i];

    public double WorkDoneAt(int i) => _workDone[i];

    /// <summary>-1 if the republic is building it, else the bloc whose firm is.</summary>
    public int ContractorAt(int i) => _contractor[i];

    /// <summary>-1 if it taps nothing, else the deposit it works.</summary>
    public int TappedAt(int i) => _tapped[i];

    public int ShiftsAt(int i) => _shifts[i];

    public double HoursAt(int i) => _hours[i];

    public double ProvisionedAt(int i) => _provisioned[i];

    public double ComfortedAt(int i) => _comforted[i];

    public double DrinkAt(int i) => _drink[i];

    /// <summary>
    /// How well the republic is serving the people who live here, component by
    /// component. Set by the contentment system, never authored, and meaningless
    /// on anything that is not housing.
    /// </summary>
    public Contentment ContentmentAt(int i) => _content[i];

    public void SetContentment(int i, Contentment c) => _content[i] = c;

    // Consequences a system writes. Never authored, which is why they are set
    // through named methods rather than by handing out the arrays.
    public void SetStaff(int i, int staff) => _staff[i] = staff;

    public void SetPowered(int i, bool powered) => _powered[i] = powered;

    public void SetHeated(int i, bool heated) => _heated[i] = heated;

    public void SetProvisioned(int i, double v) => _provisioned[i] = v;

    public void SetComforted(int i, double v) => _comforted[i] = v;

    public void SetDrink(int i, double v) => _drink[i] = v;

    public void AddWork(int i, double builderDays) => _workDone[i] += builderDays;

    public void SetContractor(int i, int market) => _contractor[i] = market;

    public void SetTapped(int i, int deposit) => _tapped[i] = deposit;

    /// <summary>The index of a building by id, or -1 if it no longer stands.</summary>
    /// <summary>
    /// Which building's footprint covers a point, or -1.
    /// </summary>
    /// <remarks>
    /// <b>The pick.</b> A screen asking "what did the player just click on" has
    /// to ask the same question placement asks, or a building can be visibly
    /// standing somewhere that cannot be selected — and the rule about what
    /// ground a building occupies is in here rather than in a renderer. A hit
    /// test written against drawn geometry would be a second copy of that rule,
    /// and the copy that drifts.
    /// <para>
    /// The last match wins, so the most recently commissioned building takes a
    /// point two of them somehow share. Footprints do not overlap — placement
    /// refuses it — so that tie-break is a statement about the impossible case
    /// rather than a policy anybody will notice.
    /// </para>
    /// </remarks>
    public int At(double x, double y)
    {
        for (var i = Count - 1; i >= 0; i--)
        {
            var half = _t.BWidth[_kind[i]] / 2.0;
            var deep = _t.BDepth[_kind[i]] / 2.0;
            if (x >= _x[i] - half && x <= _x[i] + half
                && y >= _y[i] - deep && y <= _y[i] + deep)
            {
                return _id[i];
            }
        }

        return -1;
    }

    public int IndexOf(int id)
    {
        for (var i = 0; i < _id.Count; i++)
        {
            if (_id[i] == id)
            {
                return i;
            }
        }

        return -1;
    }

    /// <summary>
    /// Put a building up as a site. It produces nothing, employs nobody and
    /// houses nobody until the work is done.
    /// </summary>
    public int Add(int kind, double x, double y)
    {
        var id = _nextId++;
        _id.Add(id);
        _kind.Add(kind);
        _x.Add(x);
        _y.Add(y);
        _priority.Add((Priority)_t.BPriority[kind]);
        _staff.Add(0);
        _powered.Add(false);
        _heated.Add(false);
        _provisioned.Add(0.0);
        _comforted.Add(0.0);
        _drink.Add(0.0);
        _content.Add(Contentment.Nothing);
        _workDone.Add(0.0);
        _contractor.Add(-1);
        _tapped.Add(-1);

        // One crew by default: a day shift is what every authored output rate
        // in the table describes.
        _shifts.Add(1);
        _hours.Add(Shifts.HoursFor(kind, id));
        Stock.Grow();
        Orders.Grow();
        return _id.Count - 1;
    }

    /// <summary>
    /// Put a building back exactly as it was, keeping its id.
    /// </summary>
    /// <remarks>
    /// Only a load calls this. <see cref="Add"/> mints the next id, which is
    /// right for a new building and wrong for one that already existed — a save
    /// that renumbered its buildings would break every journal entry and every
    /// standing order that names one.
    /// </remarks>
    internal int Restore(int id, int kind, double x, double y)
    {
        var i = Add(kind, x, y);
        _id[i] = id;

        // Ids are never reused, so the counter has to clear the highest one the
        // save carried rather than however many rows were read.
        _nextId = Math.Max(_nextId, id + 1);
        return i;
    }

    /// <summary>Pull one down. Returns false if it was not there.</summary>
    public bool Demolish(int id)
    {
        var i = IndexOf(id);
        if (i < 0)
        {
            return false;
        }

        _id.RemoveAt(i);
        _kind.RemoveAt(i);
        _x.RemoveAt(i);
        _y.RemoveAt(i);
        _priority.RemoveAt(i);
        _staff.RemoveAt(i);
        _powered.RemoveAt(i);
        _heated.RemoveAt(i);
        _provisioned.RemoveAt(i);
        _comforted.RemoveAt(i);
        _drink.RemoveAt(i);
        _content.RemoveAt(i);
        _workDone.RemoveAt(i);
        _contractor.RemoveAt(i);
        _tapped.RemoveAt(i);
        _shifts.RemoveAt(i);
        _hours.RemoveAt(i);
        Stock.RemoveAt(i);
        Orders.RemoveAt(i);
        return true;
    }

    // ---- what a building is doing ----

    public bool IsBuilt(int i) => _workDone[i] >= _t.BLabour[_kind[i]];

    /// <summary>How far along the site is, 0 to 1.</summary>
    public double Progress(int i)
    {
        var labour = _t.BLabour[_kind[i]];
        return labour <= 0.0 ? 1.0 : Math.Clamp(_workDone[i] / labour, 0.0, 1.0);
    }

    /// <summary>The share of the build still to do, 0 to 1.</summary>
    public double WorkLeft(int i)
    {
        var labour = _t.BLabour[_kind[i]];
        return labour <= 0.0 ? 0.0 : 1.0 - Math.Clamp(_workDone[i] / labour, 0.0, 1.0);
    }

    /// <summary>
    /// Whether the materials for the work <i>still to do</i> are on hand.
    /// </summary>
    /// <remarks>
    /// The bill is consumed in step with the work, so a site that has had its
    /// full bill delivered once must not then read as short of it. An earlier
    /// version demanded the whole bill at every moment, which meant a site
    /// worked for one tick and then stalled until freight topped it back up —
    /// twice the bill in total, and a build-out that crawled for a reason
    /// nothing in the construction code showed.
    /// </remarks>
    public bool HasMaterials(int i)
    {
        var kind = _kind[i];
        var left = WorkLeft(i);
        var res = _t.Materials.KeysOf(kind);
        var qty = _t.Materials.ValuesOf(kind);
        for (var m = 0; m < res.Length; m++)
        {
            if (Stock.Get(i, res[m]) + 1e-9 < qty[m] * left)
            {
                return false;
            }
        }

        return true;
    }

    /// <summary>How much of a material the site still has to be brought.</summary>
    public double MaterialOutstanding(int i, int resource)
    {
        var kind = _kind[i];
        var left = WorkLeft(i);
        var res = _t.Materials.KeysOf(kind);
        var qty = _t.Materials.ValuesOf(kind);
        var wanted = 0.0;
        for (var m = 0; m < res.Length; m++)
        {
            if (res[m] == resource)
            {
                wanted = qty[m] * left;
                break;
            }
        }

        return Math.Max(0.0, wanted - Stock.Get(i, resource));
    }

    /// <summary>How many people this place wants across all its shifts.</summary>
    public int Jobs(int i) => _t.BWorkers[_kind[i]] * _shifts[i];

    /// <summary>
    /// Hours a day this place is manned. Capped at a day, because three
    /// twelve-hour shifts is still one day — that cap is why a long shift and an
    /// extra crew are different decisions rather than two names for one
    /// multiplier.
    /// </summary>
    public double HoursCovered(int i) => Math.Min(_shifts[i] * _hours[i], 24.0);

    /// <summary>
    /// How much of a standard working day this place covers. One is one
    /// eight-hour shift, which is what every authored rate in the building table
    /// means — so a republic that never touches a roster produces exactly what
    /// it produced before shifts existed.
    /// </summary>
    public double DayShare(int i) => HoursCovered(i) / _t.StandardHours;

    /// <summary>
    /// How full the roster is, 0 to 1 — a fill fraction, not an amount of work.
    /// </summary>
    /// <remarks>
    /// The right question for anything that asks <i>whether</i> somewhere is
    /// manned, or that is capped by a physical establishment: a garage with
    /// three shifts of drivers still has only as many lorries as it owns.
    /// </remarks>
    public double Staffing(int i)
    {
        var jobs = Jobs(i);
        if (jobs == 0)
        {
            // No jobs at all — a house, a store, a pylon. It has no roster and
            // is never short-staffed. A workplace the player has closed is a
            // different thing and answers zero.
            return _t.BWorkers[_kind[i]] == 0 ? 1.0 : 0.0;
        }

        return Math.Clamp((double)_staff[i] / jobs, 0.0, 1.0);
    }

    /// <summary>
    /// Whether somebody working here travels in the dark.
    /// </summary>
    /// <remarks>
    /// A threshold rather than a daylight model, deliberately: a working period
    /// longer than this cannot fit inside daylight in any climate the republic
    /// is posted to, so no calendar, latitude or sunrise table is needed to
    /// answer the only question the labour pass asks. A place running one
    /// ordinary day shift never travels in the dark, which keeps street lighting
    /// something a republic grows into rather than a tax on the opening.
    /// </remarks>
    public bool WorksAfterDark(int i) => HoursCovered(i) > _t.DaylightHours;

    /// <summary>
    /// How much work this place does today, where 1 is one fully-staffed
    /// standard shift — what every authored rate describes.
    /// </summary>
    /// <remarks>
    /// Can exceed 1, and that is the whole point: a works running three crews
    /// round the clock makes three times as much as one running a single day
    /// shift, and it needed three times the people to do it. Other factors —
    /// power, inputs, machinery, weather — multiply this, and are deliberately
    /// not folded together, so a stalled building can always say <i>which</i>
    /// thing stalled it.
    /// </remarks>
    public double Activity(int i) => Staffing(i) * DayShare(i);

    /// <summary>The rectangle it occupies, in metres.</summary>
    public (double MinX, double MinY, double MaxX, double MaxY) Bounds(int i)
    {
        var kind = _kind[i];
        var hw = _t.BWidth[kind] / 2.0;
        var hd = _t.BDepth[kind] / 2.0;
        return (_x[i] - hw, _y[i] - hd, _x[i] + hw, _y[i] + hd);
    }

    public bool Overlaps(int a, int b)
    {
        var (ax0, ay0, ax1, ay1) = Bounds(a);
        var (bx0, by0, bx1, by1) = Bounds(b);
        return ax0 < bx1 && bx0 < ax1 && ay0 < by1 && by0 < ay1;
    }

    public double StorageCap(int i) => _t.BStorage[_kind[i]];

    /// <summary>
    /// Whether this place will take that shape of goods at all. A site under
    /// construction is exempt: a bill of materials is not stock, it is the
    /// building arriving in pieces, and a tank that could not accept the bricks
    /// it is made of could never be built.
    /// </summary>
    public bool Accepts(int i, int resource) =>
        !IsBuilt(i) || Admits(_kind[i], _t.ResourceForm[resource]);

    /// <summary>How much more of a resource this place will take in.</summary>
    public double IntakeCapacity(int i, int resource)
    {
        // A contracted site is somebody else's problem until it opens: the firm
        // brings its own materials, and a lorry sent there would be wasted.
        if (_contractor[i] >= 0 && !IsBuilt(i))
        {
            return 0.0;
        }

        if (IsBuilt(i))
        {
            if (!Admits(_kind[i], _t.ResourceForm[resource]))
            {
                return 0.0;
            }

            return _t.BStoresToOrder[_kind[i]]
                ? Math.Min(Orders.Get(i, resource), StorageCap(i))
                : StorageCap(i);
        }

        var res = _t.Materials.KeysOf(_kind[i]);
        var qty = _t.Materials.ValuesOf(_kind[i]);
        var needed = 0.0;
        for (var m = 0; m < res.Length; m++)
        {
            if (res[m] == resource)
            {
                needed = qty[m];
                break;
            }
        }

        return Math.Max(StorageCap(i), needed);
    }

    /// <summary>Whether a building of this kind will take goods of this shape.</summary>
    public bool Takes(int i, int resource) => Admits(_kind[i], _t.ResourceForm[resource]);

    private bool Admits(int kind, int form)
    {
        foreach (var f in _t.Admits.KeysOf(kind))
        {
            if (f == form)
            {
                return true;
            }
        }

        return false;
    }

    // ---- the player's decisions ----

    public void SetPriority(int id, Priority priority)
    {
        var i = IndexOf(id);
        if (i >= 0)
        {
            _priority[i] = priority;
        }
    }

    /// <summary>Zero mothballs the place; three is round the clock.</summary>
    public void SetShiftCount(int id, int shifts)
    {
        var i = IndexOf(id);
        if (i >= 0)
        {
            _shifts[i] = Math.Clamp(shifts, 0, _t.MaxShifts);
        }
    }

    public void SetNationalHours(double hours)
    {
        Shifts.SetNational(hours);
        Reroster();
    }

    public void SetKindHours(int kind, double? hours)
    {
        Shifts.SetKind(kind, hours);
        Reroster();
    }

    public void SetBuildingHours(int id, double? hours)
    {
        Shifts.SetBuilding(id, hours);
        Reroster();
    }

    /// <summary>
    /// The labour plan's running order: every workplace, most important first,
    /// ties broken by id so the order is stable across runs.
    /// </summary>
    public List<int> ByStanding()
    {
        var order = new List<int>();
        for (var i = 0; i < Count; i++)
        {
            if (_t.BWorkers[_kind[i]] > 0)
            {
                order.Add(i);
            }
        }

        order.Sort((a, b) =>
        {
            var byPriority = _priority[b].CompareTo(_priority[a]);
            return byPriority != 0 ? byPriority : _id[a].CompareTo(_id[b]);
        });
        return order;
    }

    /// <summary>How many people the republic can house.</summary>
    public int Housing()
    {
        var total = 0;
        for (var i = 0; i < Count; i++)
        {
            if (IsBuilt(i))
            {
                total += _t.BResidents[_kind[i]];
            }
        }

        return total;
    }

    private void Reroster()
    {
        for (var i = 0; i < Count; i++)
        {
            _hours[i] = Shifts.HoursFor(_kind[i], _id[i]);
        }
    }
}

/// <summary>
/// The national working day and the exceptions to it.
/// </summary>
/// <remarks>
/// <i>"Doctors work twelve, but at this hospital fourteen."</i> A rule about a
/// kind covers every such building including ones not yet built, which is the
/// difference between a policy and a batch edit; a rule about one building beats
/// it. Clearing falls back to the rule above rather than freezing whatever
/// number was in force.
/// </remarks>
public sealed class ShiftPolicy(Tables tables)
{
    private readonly Tables _t = tables;
    private readonly SortedDictionary<int, double> _byKind = [];
    private readonly SortedDictionary<int, double> _byBuilding = [];

    /// <summary>
    /// The standard everything else falls back to, and the one rule that cannot
    /// be cleared.
    /// </summary>
    public double National { get; private set; } = tables.StandardHours;

    public double? OfKind(int kind) => _byKind.TryGetValue(kind, out var h) ? h : null;

    public double? OfBuilding(int id) => _byBuilding.TryGetValue(id, out var h) ? h : null;

    public double HoursFor(int kind, int id) => OfBuilding(id) ?? OfKind(kind) ?? National;

    /// <summary>Sorted, so the interface lists rules in a stable order.</summary>
    public IReadOnlyDictionary<int, double> KindRules => _byKind;

    public IReadOnlyDictionary<int, double> BuildingRules => _byBuilding;

    public void SetNational(double hours) => National = Clamp(hours);

    public void SetKind(int kind, double? hours)
    {
        if (hours is null)
        {
            _byKind.Remove(kind);
        }
        else
        {
            _byKind[kind] = Clamp(hours.Value);
        }
    }

    public void SetBuilding(int id, double? hours)
    {
        if (hours is null)
        {
            _byBuilding.Remove(id);
        }
        else
        {
            _byBuilding[id] = Clamp(hours.Value);
        }
    }

    /// <summary>What a working day past the standard costs, in health and loyalty.</summary>
    public (double Health, double Loyalty) OverworkCost(double hours)
    {
        var over = Math.Max(0.0, hours - _t.StandardHours);
        return (over * _t.OverworkHealth, over * _t.OverworkLoyalty);
    }

    private double Clamp(double hours) =>
        double.IsNaN(hours) ? _t.StandardHours : Math.Clamp(hours, _t.MinHours, _t.MaxHours);
}
