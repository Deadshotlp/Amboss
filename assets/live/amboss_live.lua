-- Amboss Live-Bruecke 2
-- Wird von Amboss (Map-Editor) nach garrysmod/addons/amboss_live/ geschrieben.
-- Austausch ueber Dateien in garrysmod/data/amboss_live/:
--   in.txt       Befehle von Amboss (Entities aendern/erzeugen/entfernen, Inputs, Spieler versetzen)
--   brushes.txt  neue/geaenderte Brushes als begehbare Vorschau
--   start.txt    Startposition fuer "Im Spiel testen ab 3D-Kamera"
--   out.txt      Antwort an Amboss (Map, Spielerposition, letzter Befehl) - vom Server
--   alive.txt    Lebenszeichen vom Client (laeuft auch, wenn das Spiel pausiert ist)
-- In der Konsole: amboss_live_status zeigt, ob die Bruecke laeuft.
-- Laeuft nur im Einzelspieler bzw. auf dem eigenen (Listen-)Server. Zum Entfernen den Ordner
-- addons/amboss_live loeschen.

AddCSLuaFile()

if SERVER and game.IsDedicated() then return end

local DIR = "amboss_live/"
AMBOSS_LIVE = AMBOSS_LIVE or {}
local L = AMBOSS_LIVE

local function readjson(name)
	local s = file.Read(DIR .. name, "DATA")
	if not s or s == "" then return nil, s end
	return util.JSONToTable(s), s
end

local function vec(t) return Vector(tonumber(t[1]) or 0, tonumber(t[2]) or 0, tonumber(t[3]) or 0) end
local function ang(t) return Angle(tonumber(t[1]) or 0, tonumber(t[2]) or 0, tonumber(t[3]) or 0) end

-- ------------------------------------------------------------------ Vorschau-Brushes (beide Seiten)

local ENT = {}
ENT.Type = "anim"
ENT.Base = "base_anim"
ENT.PrintName = "Amboss Vorschau-Brushes"
ENT.Spawnable = false
ENT.RenderGroup = RENDERGROUP_OPAQUE

function ENT:InitHulls()
	local hulls = L.hulls
	if not hulls or #hulls == 0 then return end
	self:PhysicsInitMultiConvex(hulls)
	self:SetSolid(SOLID_VPHYSICS)
	self:SetMoveType(MOVETYPE_NONE)
	self:EnableCustomCollisions(true)
	local ph = self:GetPhysicsObject()
	if IsValid(ph) then ph:EnableMotion(false) end
	self.hull_rev = L.rev
end

function ENT:Initialize()
	self:SetRenderBounds(Vector(-32768, -32768, -32768), Vector(32768, 32768, 32768))
	self:InitHulls()
end

function ENT:Think()
	if CLIENT and self.hull_rev ~= L.rev then self:InitHulls() end
	self:NextThink(CurTime() + 0.5)
	return true
end

function ENT:Draw() end

scripted_ents.Register(ENT, "amboss_live_brushes")

local function load_brushes(t)
	L.rev = t.rev
	L.hulls = {}
	for _, b in ipairs(t.brushes or {}) do
		if b.solid and b.hull and #b.hull >= 12 then
			local h = {}
			for i = 1, #b.hull, 3 do h[#h + 1] = Vector(b.hull[i], b.hull[i + 1], b.hull[i + 2]) end
			L.hulls[#L.hulls + 1] = h
		end
	end
	if SERVER then
		if IsValid(L.brush_ent) then L.brush_ent:Remove() end
		L.brush_ent = nil
		if #L.hulls > 0 then
			local e = ents.Create("amboss_live_brushes")
			e:SetPos(Vector(0, 0, 0))
			e:Spawn()
			L.brush_ent = e
		end
		return
	end
	-- Client: Dreiecke je Material als Meshes
	for _, m in ipairs(L.meshes or {}) do m.mesh:Destroy() end
	L.meshes = {}
	L.mats = L.mats or {}
	for _, b in ipairs(t.brushes or {}) do
		for _, f in ipairs(b.faces or {}) do
			local mat = L.mats[f.mat]
			if not mat then
				local src = Material(f.mat)
				local bt = src and src:GetTexture("$basetexture")
				local name = (bt and not bt:IsError()) and bt:GetName() or "dev/dev_measuregeneric01"
				mat = CreateMaterial("amboss_live_" .. util.CRC(f.mat), "UnlitGeneric", { ["$basetexture"] = name, ["$color"] = "[0.82 0.82 0.82]" })
				L.mats[f.mat] = mat
			end
			local verts = {}
			local v = f.v or {}
			for i = 1, #v, 8 do
				verts[#verts + 1] = { pos = Vector(v[i], v[i + 1], v[i + 2]), u = v[i + 3], v = v[i + 4], normal = Vector(v[i + 5], v[i + 6], v[i + 7]) }
			end
			-- hoechstens 21.845 Dreiecke je Mesh
			for s = 1, #verts, 65535 do
				local part = {}
				for k = s, math.min(s + 65534, #verts) do part[#part + 1] = verts[k] end
				if #part >= 3 then
					local msh = Mesh()
					msh:BuildFromTriangles(part)
					L.meshes[#L.meshes + 1] = { mesh = msh, mat = mat }
				end
			end
		end
	end
end

local last_brushes = nil
timer.Create("AmbossLiveBrushes", 0.5, 0, function()
	local t, raw = readjson("brushes.txt")
	if raw == last_brushes then return end
	last_brushes = raw
	if t then load_brushes(t) else load_brushes({ brushes = {} }) end
end)

if CLIENT then
	hook.Add("PostDrawOpaqueRenderables", "AmbossLiveBrushes", function(depth, sky)
		if sky or not L.meshes then return end
		for _, m in ipairs(L.meshes) do
			render.SetMaterial(m.mat)
			m.mesh:Draw()
		end
	end)
	-- Lebenszeichen mit Echtzeit: laeuft auch, wenn das Spiel pausiert ist (Konsole/Menue offen)
	file.CreateDir("amboss_live")
	local next_alive = 0
	hook.Add("Think", "AmbossLiveAlive", function()
		if RealTime() < next_alive then return end
		next_alive = RealTime() + 0.5
		pcall(function()
			local p = LocalPlayer()
			local pos = IsValid(p) and p:GetPos() or Vector(0, 0, 0)
			file.Write(DIR .. "alive.txt", util.TableToJSON({ map = game.GetMap(), pos = { pos.x, pos.y, pos.z }, t = RealTime() }))
		end)
	end)
	print("[Amboss Live] Bruecke 2 geladen (Client)")
	return
end

-- ------------------------------------------------------------------ Server: Befehle

local last_seq = -1
local last_err = ""
local loaded = os.time() + (SysTime() % 1)
local created = {}
local by_hammer = {}

local function find_hammer(id)
	local e = by_hammer[id]
	if IsValid(e) then return e end
	by_hammer = {}
	for _, x in ipairs(ents.GetAll()) do
		local h = tonumber(x:GetKeyValues().hammerid or 0) or 0
		if h > 0 then by_hammer[h] = x end
	end
	return by_hammer[id]
end

local function player1()
	return player.GetHumans()[1]
end

local function apply(e, kv, outputs)
	for k, v in pairs(kv or {}) do
		local lk = string.lower(k)
		if lk == "origin" then e:SetPos(Vector(v))
		elseif lk == "angles" then e:SetAngles(Angle(v))
		elseif lk == "model" then e:SetModel(v)
		elseif lk == "skin" then e:SetSkin(tonumber(v) or 0)
		elseif lk == "rendercolor" then
			local c = string.Explode(" ", v)
			local a = e:GetColor().a
			e:SetColor(Color(tonumber(c[1]) or 255, tonumber(c[2]) or 255, tonumber(c[3]) or 255, a))
		elseif lk == "renderamt" then
			local col = e:GetColor()
			col.a = tonumber(v) or 255
			if col.a < 255 then e:SetRenderMode(RENDERMODE_TRANSCOLOR) end
			e:SetColor(col)
		elseif lk == "modelscale" then e:SetModelScale(tonumber(v) or 1, 0)
		elseif lk == "targetname" then e:SetName(v)
		elseif lk ~= "classname" and lk ~= "hammerid" then e:SetKeyValue(k, v)
		end
	end
	for _, o in ipairs(outputs or {}) do
		e:SetKeyValue(o[1], string.Replace(o[2], "\27", ","))
	end
end

local handlers = {}

function handlers.update(c)
	local e = c.live and created[c.id] or find_hammer(c.id)
	if not IsValid(e) then error("Entity #" .. tostring(c.id) .. " nicht im Spiel (neu kompilieren?)") end
	apply(e, c.kv, c.outputs)
end

function handlers.create(c)
	local class = c.class
	local static = class == "prop_static"
	if static then class = "prop_dynamic" end
	local e = ents.Create(class)
	if not IsValid(e) then error("Klasse " .. tostring(c.class) .. " laesst sich nicht erzeugen") end
	for k, v in pairs(c.kv or {}) do
		local lk = string.lower(k)
		if lk ~= "classname" then e:SetKeyValue(k, v) end
	end
	for _, o in ipairs(c.outputs or {}) do e:SetKeyValue(o[1], string.Replace(o[2], "\27", ",")) end
	if static then e:SetKeyValue("solid", "6") end
	e:Spawn()
	e:Activate()
	if static then
		local ph = e:GetPhysicsObject()
		if IsValid(ph) then ph:EnableMotion(false) end
	end
	created[c.id] = e
end

function handlers.remove(c)
	local e = c.live and created[c.id] or find_hammer(c.id)
	if IsValid(e) then e:Remove() end
	created[c.id] = nil
end

function handlers.fire(c)
	local n = 0
	for _, e in ipairs(ents.FindByName(c.target or "")) do
		e:Fire(c.input or "", c.param or "", 0)
		n = n + 1
	end
	if n == 0 then error("Kein Ziel namens " .. tostring(c.target)) end
end

function handlers.teleport(c)
	local p = player1()
	if not IsValid(p) then return end
	p:SetPos(vec(c.pos))
	p:SetEyeAngles(ang(c.ang))
	p:SetLocalVelocity(Vector(0, 0, 0))
end

function handlers.noclip()
	local p = player1()
	if not IsValid(p) then return end
	if p:GetMoveType() == MOVETYPE_NOCLIP then p:SetMoveType(MOVETYPE_WALK) else p:SetMoveType(MOVETYPE_NOCLIP) end
end

function handlers.map(c)
	game.ConsoleCommand("map " .. tostring(c.map) .. "\n")
end

local function run_cmds()
	local t = readjson("in.txt")
	if not t or not t.cmds then return end
	for _, c in ipairs(t.cmds) do
		local seq = tonumber(c.seq) or 0
		if seq > last_seq then
			last_seq = seq
			local h = handlers[c.op]
			if h then
				local ok, err = pcall(h, c)
				if not ok then last_err = tostring(err) end
			end
		end
	end
end

local last_in = nil
local function poll_in()
	local s = file.Read(DIR .. "in.txt", "DATA")
	if s == last_in then return end
	last_in = s
	run_cmds()
end

local VERSION = 2
local writes, write_err = 0, nil

local function status_json(p)
	local pos, a = Vector(0, 0, 0), Angle(0, 0, 0)
	if IsValid(p) then pos = p:GetPos() a = p:EyeAngles() end
	return util.TableToJSON({
		version = VERSION,
		map = game.GetMap(),
		seq = last_seq,
		pos = { pos.x, pos.y, pos.z },
		ang = { a.p, a.y, a.r },
		loaded = loaded,
		err = last_err,
	})
end

local function write_out()
	local ok, err = pcall(function() file.Write(DIR .. "out.txt", status_json(player1())) end)
	if ok then
		writes = writes + 1
	elseif write_err ~= tostring(err) then
		write_err = tostring(err)
		print("[Amboss Live] out.txt konnte nicht geschrieben werden: " .. write_err)
	end
end

local started = false
local function start()
	file.CreateDir("amboss_live")
	loaded = os.time() + (SysTime() % 1)
	-- alte Befehle aus der Zeit vor dem Map-Start ueberspringen
	local t, raw = readjson("in.txt")
	last_in = raw
	for _, c in ipairs(t and t.cmds or {}) do last_seq = math.max(last_seq, tonumber(c.seq) or 0) end
	timer.Create("AmbossLiveIn", 0.2, 0, poll_in)
	timer.Create("AmbossLiveOut", 0.5, 0, write_out)
	if not started then
		started = true
		print("[Amboss Live] Bruecke " .. VERSION .. " aktiv (Server, Map " .. game.GetMap() .. ") - Austausch ueber garrysmod/data/" .. DIR)
	end
end

hook.Add("InitPostEntity", "AmbossLive", start)
-- schon mitten in einer Map geladen (z. B. Lua neu geladen): gleich starten
if game.GetWorld and IsValid(game.GetWorld()) then start() end


if concommand then
	concommand.Add("amboss_live_status", function()
		print("[Amboss Live] Bruecke " .. VERSION .. ", gestartet: " .. tostring(started)
			.. ", out.txt geschrieben: " .. writes .. "x" .. (write_err and (", Fehler: " .. write_err) or "")
			.. ", letzter Befehl: " .. last_seq .. (last_err ~= "" and (", Fehler: " .. last_err) or ""))
		print("[Amboss Live] Ordner: garrysmod/data/" .. DIR .. "  out.txt vorhanden: " .. tostring(file.Exists and file.Exists(DIR .. "out.txt", "DATA")))
	end)
end

-- Start an der Kamera von Amboss (einmalig, nur fuer diese Map und nur kurz nach dem Kompilieren)
hook.Add("PlayerInitialSpawn", "AmbossLiveStart", function(p)
	timer.Simple(1, function()
		if not IsValid(p) then return end
		local t = readjson("start.txt")
		if not t or string.lower(t.map or "") ~= string.lower(game.GetMap()) then return end
		if os.time() - (tonumber(t.time) or 0) > 900 then return end
		file.Delete(DIR .. "start.txt")
		p:SetPos(vec(t.pos))
		p:SetEyeAngles(ang(t.ang))
	end)
end)
