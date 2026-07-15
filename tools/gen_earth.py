# Build earth.bin: 720x360 byte classes.
# 0 ocean 1 ice 2 tundra 3 boreal 4 temperate 5 steppe 6 savanna 7 desert 8 rainforest
W,H = 720,360

NA=[(-166,68),(-160,70),(-140,70),(-125,72),(-110,73),(-95,72),(-85,70),(-75,68),(-70,62),(-65,60),(-60,52),(-65,45),(-70,42),(-74,40),(-76,35),(-80,32),(-81,25),(-83,29),(-89,29),(-94,29),(-97,26),(-97,22),(-94,18),(-90,15),(-87,13),(-84,10),(-80,9),(-78,7),(-79,8),(-85,11),(-92,15),(-97,16),(-105,20),(-110,23),(-114,28),(-117,33),(-122,37),(-124,43),(-124,48),(-128,51),(-132,54),(-140,59),(-150,60),(-152,58),(-158,56),(-165,55),(-168,60)]
SA=[(-77,8),(-70,11),(-63,10),(-60,8),(-52,5),(-50,0),(-44,-2),(-38,-5),(-35,-8),(-38,-15),(-40,-22),(-48,-27),(-53,-34),(-58,-39),(-62,-41),(-65,-45),(-68,-52),(-72,-54),(-75,-48),(-73,-40),(-71,-32),(-70,-25),(-70,-18),(-75,-14),(-79,-6),(-81,-3),(-80,1),(-77,4)]
AF=[(-17,15),(-17,21),(-13,28),(-6,35),(3,37),(11,37),(19,33),(29,31),(33,29),(35,24),(37,18),(40,15),(43,11),(48,11),(51,11),(46,4),(42,-1),(40,-7),(37,-14),(35,-20),(33,-26),(28,-33),(20,-35),(18,-32),(15,-27),(12,-18),(9,-8),(9,-2),(6,4),(-2,5),(-8,4),(-13,8)]
EU=[(-9,37),(-9,43),(-2,44),(-1,47),(-5,48),(-2,50),(2,51),(5,53),(8,55),(8,57),(11,59),(5,59),(5,62),(12,66),(16,69),(22,71),(28,71),(31,70),(30,66),(24,65),(21,61),(24,59),(30,60),(29,56),(21,55),(13,54),(8,54),(4,52),(2,48),(6,45),(10,44),(13,45),(14,41),(18,40),(16,38),(13,41),(11,44),(4,43),(3,41),(-1,38),(-6,36)]
# The filled body of Europe and European Russia — the coastal EU outline
# above only traced the rim, leaving the whole interior (Germany, Poland,
# Ukraine, western Russia) as sea. This block fills it to the Urals; the
# Black Sea is carved back out below.
EUR_FILL=[(-8,39),(-6,37),(-1,38),(0,40),(3,42),(4,43),(7,44),(12,44),(13,45),(16,42),(19,40),(23,40),(28,42),(28,45),(30,46),(38,46),(40,45),(48,46),(52,50),(58,52),(60,58),(55,64),(40,64),(28,59),(24,57),(21,55),(14,54),(11,54),(8,54),(4,51),(-1,50),(-4,48),(-2,43)]
ASIA=[(31,70),(60,73),(90,76),(112,77),(130,73),(142,73),(160,70),(170,67),(178,64),(170,62),(162,59),(157,52),(152,47),(142,47),(135,44),(130,42),(126,39),(122,37),(120,30),(115,21),(109,17),(105,9),(103,2),(100,4),(98,9),(95,16),(90,21),(87,22),(84,20),(80,14),(77,7),(74,12),(71,19),(68,23),(66,25),(61,25),(57,26),(52,26),(49,28),(45,30),(42,32),(36,32),(33,34),(27,37),(26,40),(30,41),(36,42),(42,42),(48,43),(50,46),(52,50),(55,55),(58,60),(60,66),(50,68),(40,68)]
ARABIA=[(35,31),(38,26),(40,20),(43,13),(48,13),(54,17),(59,22),(58,25),(52,26),(48,29),(44,31),(39,32)]
GREEN=[(-52,60),(-43,60),(-38,65),(-30,70),(-25,71),(-32,76),(-45,78),(-58,76),(-62,72),(-55,66)]
AUS=[(114,-22),(114,-30),(116,-34),(124,-33),(130,-32),(136,-35),(140,-38),(146,-39),(150,-37),(153,-30),(153,-25),(146,-19),(142,-11),(136,-12),(132,-11),(126,-14),(122,-17)]
UK=[(-5.6,50.0),(-3.5,50.6),(1.4,51.1),(1.7,52.7),(0,53.5),(-1.5,55),(-2,57),(-4,58.6),(-6,58),(-5,55.5),(-3,54),(-4.5,53.5),(-5.3,51.8),(-4.5,51.2)]
IRE=[(-10,52),(-6,52),(-6,55),(-10,54)]
MAD=[(44,-25),(47,-16),(50,-13),(49,-20),(46,-25)]
NG=[(131,-1),(138,-2),(146,-6),(150,-9),(143,-8),(135,-4)]
BORNEO=[(109,1),(114,4),(118,1),(116,-3),(110,-2)]
SUM=[(95,5),(99,2),(104,-4),(106,-6),(102,-3),(97,2)]
JAVA=[(105,-7),(112,-8),(114,-9),(108,-8)]
JAPAN=[(130,31),(132,34),(136,35),(140,36),(141,40),(142,44),(145,44),(141,38),(136,33)]
NZ1=[(172,-34),(175,-37),(178,-38),(175,-41),(172,-38)]
NZ2=[(166,-46),(170,-43),(174,-41),(171,-44),(167,-47)]
ICELAND=[(-24,64),(-18,63),(-14,65),(-18,67),(-23,66)]
CUBA=[(-85,22),(-78,21),(-74,20),(-77,20),(-84,21)]
SRI=[(80,6),(82,8),(81,9),(79,8)]
polys=[NA,SA,AF,EU,EUR_FILL,ASIA,ARABIA,GREEN,AUS,UK,IRE,MAD,NG,BORNEO,SUM,JAVA,JAPAN,NZ1,NZ2,ICELAND,CUBA,SRI]
HUDSON=[(-95,55),(-85,55),(-80,60),(-85,64),(-92,63),(-95,58)]
# The Black Sea, carved out of the newly-filled Europe so it reads as water.
BLACK=[(27.5,41),(41,41),(41,46),(37,47),(31,46),(27.5,44)]
# Inland seas and great lakes: carved out of the land as water (class 0),
# so they read as recognizable freshwater with their own crisp shores.
GLAKES=[(-92,47),(-88,45),(-87,42),(-83,41),(-79,43),(-76,44),(-82,46),(-84,49),(-92,46)]
CASPIAN=[(47,47),(54,45),(53,40),(50,37),(48,42),(47,45)]
VICTORIA=[(31,0.5),(35,0.5),(35,-3),(31,-3)]
lakes=[GLAKES,CASPIAN,VICTORIA]

def inside(poly, lon, lat):
    n=len(poly); j=n-1; c=False
    for i in range(n):
        xi,yi=poly[i]; xj,yj=poly[j]
        if ((yi>lat)!=(yj>lat)) and (lon < (xj-xi)*(lat-yi)/(yj-yi+1e-9)+xi):
            c=not c
        j=i
    return c

def box(la,lo,la0,la1,lo0,lo1): return la0<=la<=la1 and lo0<=lo<=lo1

import struct, random
random.seed(7)

def jit(x, y, k):
    # Deterministic per-texel jitter so climate borders fray naturally
    # instead of following ruler lines. Coastlines stay unjittered.
    h = (x*374761393 + y*668265263 + k*2246822519) & 0xffffffff
    h = (h ^ (h >> 13)) * 1274126177 & 0xffffffff
    return ((h >> 9) & 1023)/1023.0 - 0.5

data=bytearray(W*H)
for y in range(H):
    lat = 90.0 - (y+0.5)*180.0/H
    for x in range(W):
        lon = -180.0 + (x+0.5)*360.0/W
        land = any(inside(p,lon,lat) for p in polys)
        if inside(HUDSON,lon,lat): land=False
        if inside(BLACK,lon,lat): land=False
        if any(inside(p,lon,lat) for p in lakes): land=False
        if lat < -66: land=True   # Antarctica
        c=0
        if land:
            # Fray every climate border by up to ~1.5 degrees.
            jla = lat + jit(x,y,1)*3.0
            jlo = lon + jit(x,y,2)*3.0
            a=abs(jla)
            c=4
            if lat<-60 or a>=72 or (inside(GREEN,jlo,jla) and jla>62): c=1
            elif a>=64: c=2
            elif a>=50: c=3
            elif a>=35: c=4
            elif a>=23: c=5
            else: c=6
            # real-world overrides
            if box(jla,jlo,16,30,-15,33): c=7      # Sahara
            elif box(jla,jlo,13,30,36,58): c=7     # Arabia
            elif box(jla,jlo,36,46,80,112): c=7    # Gobi/Taklamakan
            elif box(jla,jlo,24,30,66,75): c=7     # Thar
            elif box(jla,jlo,-30,-17,12,25): c=7   # Kalahari
            elif box(jla,jlo,-31,-19,118,143): c=7 # Outback
            elif box(jla,jlo,22,37,-117,-102): c=7 # Sonora/Mojave
            elif box(jla,jlo,-27,-17,-71,-68): c=7 # Atacama
            elif box(jla,jlo,-15,4,-75,-48): c=8   # Amazon
            elif box(jla,jlo,-6,6,10,30): c=8      # Congo
            elif box(jla,jlo,-9,13,92,152): c=8    # SE Asia / Indonesia
            elif box(jla,jlo,4,18,-93,-77): c=8    # Central America
            elif box(jla,jlo,38,50,-104,-93): c=5  # Great Plains
            elif box(jla,jlo,42,52,58,92): c=5     # Kazakh steppe
            elif box(jla,jlo,24,36,-98,-72): c=4   # humid American Southeast
            elif box(jla,jlo,21,35,98,125): c=4    # subtropical East China
        data[y*W+x]=c
open('crates/ods-app/assets/earth.bin','wb').write(bytes(data))
# ASCII preview
CH=' *^%#.-~sR'
for y in range(0,H,10):
    row=''.join(CH[data[y*W+x]] for x in range(0,W,8))
    print(row)
