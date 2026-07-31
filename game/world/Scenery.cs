using System;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.World;

/// <summary>
/// The republic as something you can look at: ground, sky, sun and a camera.
/// </summary>
/// <remarks>
/// <para>
/// Built in code rather than authored as a scene, because almost all of it
/// depends on the map: the mesh is the terrain's heightmap, the camera opens on
/// where the posting actually is, and the shadow range is a function of the boom
/// length. A <c>.tscn</c> would carry the half that does not vary and hide the
/// half that does.
/// </para>
/// <para>
/// <b>Nothing here decides anything.</b> It reads the simulation and draws it.
/// </para>
/// </remarks>
public sealed partial class Scenery : Node3D
{
    private readonly Look _look;
    private ShaderMaterial _ground = null!;
    private Buildings _buildings = null!;
    private DirectionalLight3D _sun = null!;
    private CameraRig _rig = null!;

    public Scenery(Look look) => _look = look;

    /// <summary>Stand the world up around a republic.</summary>
    public void Raise(Sim.World world, double atX, double atZ)
    {
        ArgumentNullException.ThrowIfNull(world);

        Ground(world.Terrain);

        _buildings = new Buildings { Name = "Buildings" };
        AddChild(_buildings);
        _buildings.Raise(world.Tables, world.Terrain);

        Sky();
        Sun();
        Camera((float)world.Terrain.Extent, (float)atX, (float)atZ);
    }

    /// <summary>
    /// Where on the ground a screen position points, or <c>null</c> if it misses
    /// the map.
    /// </summary>
    /// <remarks>
    /// Marched along the ray rather than intersected analytically, because the
    /// ground is a heightmap and not a plane: a plane test puts a building on the
    /// far side of a hill from where the player clicked, and does it silently.
    /// The step is the terrain's own cell size, so the search is as fine as the
    /// map it is searching.
    /// </remarks>
    public (double X, double Y)? GroundUnder(Viewport viewport, Vector2 at, Terrain terrain)
    {
        ArgumentNullException.ThrowIfNull(viewport);
        ArgumentNullException.ThrowIfNull(terrain);

        var camera = viewport.GetCamera3D();
        if (camera is null)
        {
            return null;
        }

        var from = camera.ProjectRayOrigin(at);
        var along = camera.ProjectRayNormal(at);
        var step = (float)terrain.CellSize;

        for (var travelled = 0.0f; travelled < camera.Far; travelled += step)
        {
            var here = from + (along * travelled);
            if (!terrain.Contains(here.X, here.Z))
            {
                continue;
            }

            if (here.Y <= TerrainMesh.GroundAt(terrain, here.X, here.Z))
            {
                return (here.X, here.Z);
            }
        }

        return null;
    }

    /// <summary>Put the buildings where the republic says they are.</summary>
    public void Refresh(Sim.World world) => _buildings.Refresh(world);

    /// <summary>Aim the camera. Capture runs use this.</summary>
    public void Aim(float pitchDegrees, float distance)
    {
        _rig.SetPitch(pitchDegrees);
        _rig.SetDistance(distance);
    }

    private void Ground(Terrain terrain)
    {
        var mesh = new ArrayMesh();
        mesh.AddSurfaceFromArrays(Mesh.PrimitiveType.Triangles, TerrainMesh.Surface(terrain));

        // Vertex colour carries the surface kind — red grass, green forest, blue
        // rock, black water — and the shader decides what each looks like. The
        // art direction lives there rather than in the mesh builder.
        _ground = new ShaderMaterial { Shader = GD.Load<Shader>("res://world/terrain.gdshader") };

        foreach (var surface in new[] { "grass", "forest", "rock", "shore" })
        {
            _ground.SetShaderParameter($"{surface}_colour", Texture(surface, "colour"));
            _ground.SetShaderParameter($"{surface}_normal", Texture(surface, "normal"));
            _ground.SetShaderParameter($"{surface}_rough", Texture(surface, "roughness"));
        }

        _ground.SetShaderParameter("snow_colour", Texture("snow", "colour"));
        _ground.SetShaderParameter("snow_normal", Texture("snow", "normal"));
        _ground.SetShaderParameter("water_colour", _look.Water);
        _ground.SetShaderParameter("contour_strength", _look.ContourStrength);
        _ground.SetShaderParameter("map_extent", (float)terrain.Extent);

        AddChild(new MeshInstance3D
        {
            Name = "Ground",
            Mesh = mesh,
            MaterialOverride = _ground,
        });
    }

    /// <summary>
    /// One surface map, imported by Godot from <c>art/textures</c>.
    /// </summary>
    /// <remarks>
    /// The colour maps are sRGB and the normal and roughness maps are not, and
    /// getting that wrong is quiet: an sRGB-decoded normal map still looks like a
    /// normal map, it just lights everything at the wrong angle. The importer is
    /// what settles it, from the <c>.import</c> files beside the images.
    /// </remarks>
    private static Texture2D? Texture(string surface, string channel) =>
        ResourceLoader.Exists($"res://art/textures/{surface}_{channel}.jpg")
            ? GD.Load<Texture2D>($"res://art/textures/{surface}_{channel}.jpg")
            : null;

    private void Sky()
    {
        var sky = new ShaderMaterial { Shader = GD.Load<Shader>("res://world/sky.gdshader") };
        sky.SetShaderParameter("zenith_colour", _look.SkyTop);
        sky.SetShaderParameter("horizon_colour", _look.SkyHorizon);
        sky.SetShaderParameter("ground_colour", _look.GroundHorizon);
        sky.SetShaderParameter("cloud_cover", _look.CloudCover);

        var environment = new Godot.Environment
        {
            BackgroundMode = Godot.Environment.BGMode.Sky,
            Sky = new Sky { SkyMaterial = sky },
            AmbientLightSource = Godot.Environment.AmbientSource.Sky,
            AmbientLightSkyContribution = 1.0f,
            AmbientLightColor = _look.AmbientColour,
            AmbientLightEnergy = _look.AmbientEnergy,
            FogEnabled = true,
            FogMode = Godot.Environment.FogModeEnum.Exponential,
            FogLightColor = _look.FogColour,
            FogDensity = _look.FogDensity,

            // <b>Fog tints the sky by default, and at full strength it drowns
            // it.</b> A scattering sky rendered behind a full-strength fog is a
            // flat sheet of fog colour with the sun somewhere in it — which
            // looks exactly like a sky shader that failed to compile, and is
            // how one gets pronounced broken.
            FogSkyAffect = 0.18f,

            // Distant ground takes the sky's colour rather than only the fog's,
            // which is what makes a far hill read as far rather than as pale.
            FogAerialPerspective = 0.45f,

            TonemapMode = Godot.Environment.ToneMapper.Aces,
            TonemapWhite = 6.0f,

            // What stops a shadowed wall going flat grey, and what puts a
            // crease where two surfaces meet.
            SsaoEnabled = true,
            SsaoRadius = 12.0f,
            SsaoIntensity = 2.0f,
            SsilEnabled = true,
            SsilRadius = 24.0f,
        };

        AddChild(new WorldEnvironment { Name = "Weather", Environment = environment });
    }

    private void Sun()
    {
        _sun = new DirectionalLight3D
        {
            Name = "Sun",
            LightColor = _look.SunColour,
            LightEnergy = _look.SunEnergy,
            ShadowEnabled = true,
        };

        // Elevation above the horizon and a compass bearing, rather than a
        // rotation nobody can picture.
        _sun.RotationDegrees = new Vector3(-_look.SunElevation, _look.SunAzimuth, 0.0f);
        AddChild(_sun);
    }

    private void Camera(float extent, float atX, float atZ)
    {
        _rig = new CameraRig { Name = "Rig" };
        _rig.AddChild(new Camera3D
        {
            Name = "Camera",

            // A six-kilometre map seen from a kilometre up needs the far plane
            // out past the far corner, or the horizon is a hard edge where the
            // world stops.
            Far = extent * 3.0f,
            Near = 1.0f,
        });

        // <b>The shadow range is measured from the camera and the light cannot
        // work it out for itself.</b> It defaults to a hundred metres, and this
        // camera opens a kilometre out — so without this nothing in frame is ever
        // inside the range, not one shadow is drawn, and nothing errors.
        _rig.DistanceChanged += metres =>
            _sun.DirectionalShadowMaxDistance = Mathf.Max(400.0f, metres * 2.5f);

        AddChild(_rig);
        _rig.FrameMap(extent, atX, atZ);
    }
}
