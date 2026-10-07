#pragma once

#include "ofMain.h"
#include "ofxOsc.h"
#include "ofxImGui.h"


class ofApp : public ofBaseApp{

	public:
		void setup() override;
		void update() override;
		void draw() override;
		void exit() override;

		void drawFoot(glm::vec2 _pos, glm::vec2 _dir, float _size=0.2);//向きがわかる足を描画する

		void checkFootLife();//足の寿命をチェックして古いものを削除する
		void checkFootStampLife();//足のスタンプの寿命をチェックして古いものを削除する

		void saveConfig();//検出パラメータをconfig.jsonに保存する
		void loadConfig();//config.jsonから検出パラメータを読み込む

    ofxOscReceiver rev;
	ofxOscSender sender;

	//検出パラメータ
	float lifeTime         = 1;    //足の寿命[s]
	float matchWindow      = 0.1;  //連続とみなす時間[s]
	float matchMaxDist     = 1;    //連続とみなす距離[m]
	float landingSpeed     = 0.8;  //着地とみなす速度[m/s]
	float directionWindow  = 0.5;  //足跡の向きを決定するために参照する過去の足の時間[s]
	float stampLifeTime    = 2;    //足跡スタンプの寿命[s]
	float prevStampWindow  = 2.0;  //直前の足跡を探す時間[s]
	float prevStampMinDist = 0.25; //直前の足跡とみなす最小距離[m]
	float prevStampMaxDist = 2;  //直前の足跡とみなす最大距離[m]
	float blockerRadius    = 0.2;  //足跡設置禁止領域の半径[m]
	float blockerLifeTime  = 0.15; //足跡設置禁止領域の寿命[s]

	struct Foot {
		glm::vec2 pos;
		glm::vec2 vel;
		uint64_t timeStamp=0;
		pair<uint64_t,int> prevFootAddress=make_pair(0,-1);//連続とみなすひとつ前の足の住所
		float area=0;
	};
	map<uint64_t,vector<Foot>> footHistory;

	const Foot & findOldestLinkedFoot(const Foot & _start,float _window);

	enum FootSide{
		LEFT=-1,
		UNDEFINED=0,
		RIGHT=1
	};

	struct FootStamp{
		glm::vec2 pos;
		glm::vec2 dir;
		pair<uint64_t,int> prevStampAddress=make_pair(0,-1);//連続とみなすひとつ前の足跡の住所
		int side=UNDEFINED;
	};
	map<uint64_t,vector<FootStamp>> footStampHistory;

	// history の中から、now より前の window[s] 以内で、pos から minDist〜maxDist にある最も近い要素の住所を返す
// 見つからなければ (0,-1)
	template<typename T>
	pair<uint64_t,int> findNearestInWindow(
		const map<uint64_t,vector<T>> & _history,
		glm::vec2 _pos, 
		uint64_t _now,
		float _window, 
		float _minDist, 
		float _maxDist
	);


	struct Blocker{//足跡設置禁止領域
		glm::vec2 pos;
		uint64_t lastSeen = 0;
	};
	vector<Blocker> blockers;

	string distIp="localhost";
	int distPort=10002;
	int listenPort=10001;

	ofxImGui::Gui gui;

	void sendFootStampOSC(const FootStamp & _fs);//足跡スタンプをOSCで送信する

};
